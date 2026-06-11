use std::cell::RefCell;
use windows::core::{Interface, BSTR, VARIANT};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Accessibility::{
    AutomationElementMode_Full, CUIAutomation, ExpandCollapseState_Collapsed, IUIAutomation,
    IUIAutomationCacheRequest, IUIAutomationElement, IUIAutomationExpandCollapsePattern,
    IUIAutomationInvokePattern, IUIAutomationRangeValuePattern, IUIAutomationScrollItemPattern,
    IUIAutomationScrollPattern, IUIAutomationSelectionItemPattern, IUIAutomationSelectionPattern,
    IUIAutomationTextPattern, IUIAutomationTogglePattern, IUIAutomationTreeWalker,
    IUIAutomationValuePattern, IUIAutomationVirtualizedItemPattern, ScrollAmount_LargeDecrement,
    ScrollAmount_LargeIncrement, ScrollAmount_NoAmount, TreeScope_Subtree,
    UIA_AutomationIdPropertyId, UIA_BoundingRectanglePropertyId, UIA_ControlTypePropertyId,
    UIA_ExpandCollapsePatternId, UIA_InvokePatternId, UIA_IsEnabledPropertyId,
    UIA_IsOffscreenPropertyId, UIA_ItemContainerPatternId, UIA_NamePropertyId,
    UIA_RangeValuePatternId, UIA_ScrollItemPatternId, UIA_ScrollPatternId,
    UIA_SelectionItemPatternId, UIA_SelectionPatternId, UIA_TextPatternId, UIA_TogglePatternId,
    UIA_ValuePatternId, UIA_VirtualizedItemPatternId,
};

/// One cached element: the live UIA element plus its screen rectangle.
struct Cached {
    elem: IUIAutomationElement,
    rect: Option<(i32, i32, i32, i32)>,
}

thread_local! {
    static AUTOMATION: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
    static REGISTRY: RefCell<Vec<Cached>> = const { RefCell::new(Vec::new()) };
}

fn automation() -> Result<IUIAutomation, String> {
    AUTOMATION.with(|cell| {
        if let Some(a) = cell.borrow().as_ref() {
            return Ok(a.clone());
        }
        let auto: IUIAutomation = unsafe {
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("CoCreateInstance(CUIAutomation) failed: {e}"))?
        };
        *cell.borrow_mut() = Some(auto.clone());
        Ok(auto)
    })
}

fn not_null<T: Interface>(obj: &T) -> bool {
    !obj.as_raw().is_null()
}

fn bstr(s: windows::core::Result<BSTR>) -> String {
    s.map(|b| b.to_string()).unwrap_or_default()
}

fn control_type_name(id: i32) -> &'static str {
    match id {
        50000 => "Button",
        50002 => "CheckBox",
        50003 => "ComboBox",
        50004 => "Edit",
        50005 => "Hyperlink",
        50006 => "Image",
        50007 => "ListItem",
        50008 => "List",
        50009 => "Menu",
        50010 => "MenuBar",
        50011 => "MenuItem",
        50012 => "ProgressBar",
        50013 => "RadioButton",
        50014 => "ScrollBar",
        50015 => "Slider",
        50016 => "Spinner",
        50017 => "StatusBar",
        50018 => "Tab",
        50019 => "TabItem",
        50020 => "Text",
        50021 => "ToolBar",
        50022 => "ToolTip",
        50023 => "Tree",
        50024 => "TreeItem",
        50025 => "Custom",
        50026 => "Group",
        50027 => "Thumb",
        50028 => "DataGrid",
        50029 => "DataItem",
        50030 => "Document",
        50031 => "SplitButton",
        50032 => "Window",
        50033 => "Pane",
        50034 => "Header",
        50035 => "HeaderItem",
        50036 => "Table",
        50037 => "TitleBar",
        50038 => "Separator",
        50039 => "SemanticZoom",
        50040 => "AppBar",
        _ => "Control",
    }
}

fn control_type_id(name: &str) -> Option<i32> {
    match name
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '_', '-'], "")
        .as_str()
    {
        "button" => Some(50000),
        "checkbox" => Some(50002),
        "combobox" => Some(50003),
        "edit" | "textbox" => Some(50004),
        "hyperlink" | "link" => Some(50005),
        "image" => Some(50006),
        "listitem" => Some(50007),
        "list" => Some(50008),
        "menu" => Some(50009),
        "menubar" => Some(50010),
        "menuitem" => Some(50011),
        "progressbar" => Some(50012),
        "radiobutton" => Some(50013),
        "scrollbar" => Some(50014),
        "slider" => Some(50015),
        "spinner" => Some(50016),
        "statusbar" => Some(50017),
        "tab" => Some(50018),
        "tabitem" => Some(50019),
        "text" => Some(50020),
        "toolbar" => Some(50021),
        "tooltip" => Some(50022),
        "tree" => Some(50023),
        "treeitem" => Some(50024),
        "custom" => Some(50025),
        "group" => Some(50026),
        "thumb" => Some(50027),
        "datagrid" => Some(50028),
        "dataitem" => Some(50029),
        "document" => Some(50030),
        "splitbutton" => Some(50031),
        "window" => Some(50032),
        "pane" => Some(50033),
        "header" => Some(50034),
        "headeritem" => Some(50035),
        "table" => Some(50036),
        "titlebar" => Some(50037),
        "separator" => Some(50038),
        "semanticzoom" => Some(50039),
        "appbar" => Some(50040),
        _ => None,
    }
}

fn elem_rect(elem: &IUIAutomationElement) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let r = elem.CurrentBoundingRectangle().ok()?;
        if r.right <= r.left || r.bottom <= r.top {
            return None;
        }
        Some((r.left, r.top, r.right, r.bottom))
    }
}

/// Build a cache request that bulk-fetches every property we render for the whole
/// subtree in a single cross-process call. ``AutomationElementMode_Full`` keeps
/// live references, so cached elements remain actionable (SetFocus / Invoke /
/// SetValue) afterwards. Patterns are cached too so we can show supported actions
/// without extra cross-process calls.
fn make_cache_request(auto: &IUIAutomation) -> Result<IUIAutomationCacheRequest, String> {
    unsafe {
        let cr = auto
            .CreateCacheRequest()
            .map_err(|e| format!("CreateCacheRequest failed: {e}"))?;
        cr.SetTreeScope(TreeScope_Subtree).ok();
        cr.SetAutomationElementMode(AutomationElementMode_Full).ok();
        for pid in [
            UIA_NamePropertyId,
            UIA_ControlTypePropertyId,
            UIA_BoundingRectanglePropertyId,
            UIA_AutomationIdPropertyId,
            UIA_IsEnabledPropertyId,
            UIA_IsOffscreenPropertyId,
        ] {
            cr.AddProperty(pid).ok();
        }
        for pat in [
            UIA_InvokePatternId,
            UIA_ValuePatternId,
            UIA_RangeValuePatternId,
            UIA_TogglePatternId,
            UIA_ExpandCollapsePatternId,
            UIA_SelectionItemPatternId,
            UIA_ScrollPatternId,
            UIA_ScrollItemPatternId,
            UIA_VirtualizedItemPatternId,
            UIA_ItemContainerPatternId,
        ] {
            cr.AddPattern(pat).ok();
        }
        Ok(cr)
    }
}

/// Supported action labels for a cached element (read from the cache; no
/// cross-process calls). Mirrors the labels used by the Python `observe` tool.
fn cached_patterns(elem: &IUIAutomationElement) -> Vec<&'static str> {
    let checks = [
        (UIA_InvokePatternId, "invoke"),
        (UIA_ValuePatternId, "value"),
        (UIA_RangeValuePatternId, "range"),
        (UIA_TogglePatternId, "toggle"),
        (UIA_ExpandCollapsePatternId, "expandcollapse"),
        (UIA_SelectionItemPatternId, "select"),
        (UIA_ScrollPatternId, "scroll"),
        (UIA_ScrollItemPatternId, "scrollitem"),
        (UIA_VirtualizedItemPatternId, "virtualized"),
        (UIA_ItemContainerPatternId, "itemcontainer"),
    ];
    let mut out = Vec::new();
    for (pid, label) in checks {
        unsafe {
            if let Ok(p) = elem.GetCachedPattern(pid) {
                if !p.as_raw().is_null() {
                    out.push(label);
                }
            }
        }
    }
    out
}

fn cached_rect(elem: &IUIAutomationElement) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let r = elem.CachedBoundingRectangle().ok()?;
        if r.right <= r.left || r.bottom <= r.top {
            return None;
        }
        Some((r.left, r.top, r.right, r.bottom))
    }
}

/// Describe an element using only cached properties (no cross-process calls).
/// ``detailed`` adds the automation id (often a noisy GUID); patterns are always
/// shown because they tell the agent which action to use.
fn describe_cached(
    elem: &IUIAutomationElement,
    idx: usize,
    depth: usize,
    detailed: bool,
) -> (String, Option<(i32, i32, i32, i32)>) {
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!("[{idx}]"));
    let ct = unsafe { elem.CachedControlType().map(|c| c.0).unwrap_or(0) };
    parts.push(control_type_name(ct).to_string());
    let name = unsafe { bstr(elem.CachedName()) };
    if !name.is_empty() {
        let trimmed: String = name.chars().take(80).collect();
        parts.push(format!("\"{trimmed}\""));
    }
    if detailed {
        let aid = unsafe { bstr(elem.CachedAutomationId()) };
        if !aid.is_empty() {
            parts.push(format!("#{aid}"));
        }
    }
    let enabled = unsafe { elem.CachedIsEnabled().map(|b| b.as_bool()).unwrap_or(true) };
    if !enabled {
        parts.push("(disabled)".to_string());
    }
    let rect = cached_rect(elem);
    if let Some((l, t, r, b)) = rect {
        parts.push(format!("@{},{}", (l + r) / 2, (t + b) / 2));
    }
    let pats = cached_patterns(elem);
    if !pats.is_empty() {
        parts.push(format!("{{{}}}", pats.join(",")));
    }
    let indent = "  ".repeat(depth);
    (format!("{indent}{}", parts.join(" ")), rect)
}

fn describe(
    elem: &IUIAutomationElement,
    idx: usize,
    depth: usize,
    detailed: bool,
) -> (String, Option<(i32, i32, i32, i32)>) {
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!("[{idx}]"));
    let ct = unsafe { elem.CurrentControlType().map(|c| c.0).unwrap_or(0) };
    parts.push(control_type_name(ct).to_string());
    let name = unsafe { bstr(elem.CurrentName()) };
    if !name.is_empty() {
        let trimmed: String = name.chars().take(80).collect();
        parts.push(format!("\"{trimmed}\""));
    }
    if detailed {
        let aid = unsafe { bstr(elem.CurrentAutomationId()) };
        if !aid.is_empty() {
            parts.push(format!("#{aid}"));
        }
    }
    let enabled = unsafe { elem.CurrentIsEnabled().map(|b| b.as_bool()).unwrap_or(true) };
    if !enabled {
        parts.push("(disabled)".to_string());
    }
    let rect = elem_rect(elem);
    if let Some((l, t, r, b)) = rect {
        parts.push(format!("@{},{}", (l + r) / 2, (t + b) / 2));
    }
    let indent = "  ".repeat(depth);
    (format!("{indent}{}", parts.join(" ")), rect)
}

#[allow(clippy::too_many_arguments)]
fn walk(
    walker: &IUIAutomationTreeWalker,
    elem: &IUIAutomationElement,
    depth: usize,
    max_depth: usize,
    max_elements: usize,
    detailed: bool,
    lines: &mut Vec<String>,
    reg: &mut Vec<Cached>,
) {
    if reg.len() >= max_elements {
        return;
    }
    let idx = reg.len();
    let (line, rect) = describe(elem, idx, depth, detailed);
    lines.push(line);
    reg.push(Cached {
        elem: elem.clone(),
        rect,
    });

    if depth >= max_depth {
        return;
    }
    unsafe {
        let mut child = match walker.GetFirstChildElement(elem) {
            Ok(c) if not_null(&c) => c,
            _ => return,
        };
        loop {
            if reg.len() >= max_elements {
                break;
            }
            walk(
                walker,
                &child,
                depth + 1,
                max_depth,
                max_elements,
                detailed,
                lines,
                reg,
            );
            child = match walker.GetNextSiblingElement(&child) {
                Ok(n) if not_null(&n) => n,
                _ => break,
            };
        }
    }
}

/// Walk the cached subtree (all in-process; no cross-process calls).
fn walk_cached(
    elem: &IUIAutomationElement,
    depth: usize,
    max_depth: usize,
    max_elements: usize,
    detailed: bool,
    lines: &mut Vec<String>,
    reg: &mut Vec<Cached>,
) {
    if reg.len() >= max_elements {
        return;
    }
    let idx = reg.len();
    let (line, rect) = describe_cached(elem, idx, depth, detailed);
    lines.push(line);
    reg.push(Cached {
        elem: elem.clone(),
        rect,
    });

    if depth >= max_depth {
        return;
    }
    let children = match unsafe { elem.GetCachedChildren() } {
        Ok(arr) => arr,
        Err(_) => return,
    };
    let count = unsafe { children.Length().unwrap_or(0) };
    for i in 0..count {
        if reg.len() >= max_elements {
            break;
        }
        if let Ok(child) = unsafe { children.GetElement(i) } {
            walk_cached(
                &child,
                depth + 1,
                max_depth,
                max_elements,
                detailed,
                lines,
                reg,
            );
        }
    }
}

/// Result of building a window's accessibility state.
pub struct TreeResult {
    pub tree: String,
    pub focused_element: Option<String>,
    pub selected_text: Option<String>,
    pub selected_elements: Option<Vec<String>>,
    pub document_text: Option<String>,
}

/// Read selection/document text + selected items from the focused element and the
/// window root (best effort; any failure yields None). cap bounds document
/// text length so a huge document doesn't blow up the response.
fn extract_selection(
    auto: &IUIAutomation,
    root: &IUIAutomationElement,
    cap: usize,
) -> (Option<String>, Option<Vec<String>>, Option<String>) {
    let mut selected_text: Option<String> = None;
    let mut document_text: Option<String> = None;
    let mut selected_elements: Option<Vec<String>> = None;

    unsafe {
        // Selection + document text come from the focused element's TextPattern.
        if let Ok(focused) = auto.GetFocusedElement() {
            if not_null(&focused) {
                if let Ok(p) = focused.GetCurrentPattern(UIA_TextPatternId) {
                    if !p.as_raw().is_null() {
                        if let Ok(tp) = p.cast::<IUIAutomationTextPattern>() {
                            // Selected text (first range).
                            if let Ok(ranges) = tp.GetSelection() {
                                if let Ok(len) = ranges.Length() {
                                    let mut acc = String::new();
                                    for i in 0..len {
                                        if let Ok(r) = ranges.GetElement(i) {
                                            if let Ok(s) = r.GetText(cap as i32) {
                                                acc.push_str(&s.to_string());
                                            }
                                        }
                                    }
                                    let acc = acc.trim().to_string();
                                    if !acc.is_empty() {
                                        selected_text = Some(acc);
                                    }
                                }
                            }
                            // Whole-document text.
                            if let Ok(doc) = tp.DocumentRange() {
                                if let Ok(s) = doc.GetText(cap as i32) {
                                    let s = s.to_string();
                                    let s = s.trim();
                                    if !s.is_empty() {
                                        document_text = Some(s.chars().take(cap).collect());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Selected items come from a SelectionPattern container (lists, grids, tabs).
        if let Ok(p) = root.GetCurrentPattern(UIA_SelectionPatternId) {
            if !p.as_raw().is_null() {
                if let Ok(sp) = p.cast::<IUIAutomationSelectionPattern>() {
                    if let Ok(sel) = sp.GetCurrentSelection() {
                        if let Ok(len) = sel.Length() {
                            let mut items = Vec::new();
                            for i in 0..len.min(20) {
                                if let Ok(el) = sel.GetElement(i) {
                                    let ct = el.CurrentControlType().map(|c| c.0).unwrap_or(0);
                                    let name = bstr(el.CurrentName());
                                    items.push(format!("{} \"{}\"", control_type_name(ct), name));
                                }
                            }
                            if !items.is_empty() {
                                selected_elements = Some(items);
                            }
                        }
                    }
                }
            }
        }
    }
    (selected_text, selected_elements, document_text)
}

/// Walk a window's control view, rebuilding the element registry, and return
/// the formatted text tree.
///
/// Uses UI Automation bulk caching one BuildUpdatedCache call fetches every
/// rendered property for the whole subtree in a single cross-process round-trip,
/// then the tree is walked entirely in-process. This is dramatically faster than
/// reading each property live (which costs a cross-process call apiece). Falls
/// back to a live TreeWalker walk if caching is unavailable.
pub fn build_window_tree(
    hwnd: HWND,
    max_elements: usize,
    max_depth: usize,
    detailed: bool,
) -> Result<TreeResult, String> {
    let auto = automation()?;
    let root = unsafe {
        auto.ElementFromHandle(hwnd)
            .map_err(|e| format!("ElementFromHandle failed: {e}"))?
    };
    if !not_null(&root) {
        return Err("window has no UI Automation element".into());
    }
    Ok(build_tree(&auto, &root, max_elements, max_depth, detailed))
}

/// Build an indexed tree for a scope: a window handle, or the whole desktop
/// (`scope = None`). Populates the element registry like `observe` does.
pub fn get_ui_tree(
    scope: Option<i64>,
    max_elements: usize,
    max_depth: usize,
    detailed: bool,
) -> Result<TreeResult, String> {
    let auto = automation()?;
    let root = match scope {
        Some(id) => unsafe {
            auto.ElementFromHandle(HWND(id as *mut std::ffi::c_void))
                .map_err(|e| format!("ElementFromHandle failed: {e}"))?
        },
        None => unsafe {
            auto.GetRootElement()
                .map_err(|e| format!("GetRootElement failed: {e}"))?
        },
    };
    if !not_null(&root) {
        return Err("scope has no UI Automation element".into());
    }
    Ok(build_tree(&auto, &root, max_elements, max_depth, detailed))
}

fn build_tree(
    auto: &IUIAutomation,
    root: &IUIAutomationElement,
    max_elements: usize,
    max_depth: usize,
    detailed: bool,
) -> TreeResult {
    let mut lines: Vec<String> = Vec::new();
    let mut reg: Vec<Cached> = Vec::new();

    let cached_ok = match make_cache_request(auto) {
        Ok(cr) => match unsafe { root.BuildUpdatedCache(&cr) } {
            Ok(cached_root) if not_null(&cached_root) => {
                walk_cached(
                    &cached_root,
                    0,
                    max_depth,
                    max_elements,
                    detailed,
                    &mut lines,
                    &mut reg,
                );
                true
            }
            _ => false,
        },
        Err(_) => false,
    };

    if !cached_ok {
        // Fallback: live tree walk (slower; one cross-process call per property).
        lines.clear();
        reg.clear();
        if let Ok(walker) = unsafe { auto.ControlViewWalker() } {
            walk(
                &walker,
                root,
                0,
                max_depth,
                max_elements,
                detailed,
                &mut lines,
                &mut reg,
            );
        }
    }

    // Signal truncation so the agent knows the tree was capped and can narrow
    // scope or raise the limit rather than assuming it saw everything.
    if reg.len() >= max_elements {
        lines.push(format!(
            "... (truncated at {max_elements} elements; act on what is shown, scroll, or raise max_elements)"
        ));
    }

    // Focused element (best effort).
    let focused = unsafe {
        auto.GetFocusedElement().ok().and_then(|f| {
            if not_null(&f) {
                let ct = f.CurrentControlType().map(|c| c.0).unwrap_or(0);
                let name = bstr(f.CurrentName());
                Some(format!("{} \"{}\"", control_type_name(ct), name))
            } else {
                None
            }
        })
    };

    // Selection + document text (best effort).
    let (selected_text, selected_elements, document_text) = extract_selection(auto, root, 8192);

    REGISTRY.with(|r| *r.borrow_mut() = reg);
    TreeResult {
        tree: lines.join("\n"),
        focused_element: focused,
        selected_text,
        selected_elements,
        document_text,
    }
}

/// Describe one live element as JSON (control type, name, automation id, center).
fn describe_json(elem: &IUIAutomationElement) -> serde_json::Value {
    let ct = unsafe { elem.CurrentControlType().map(|c| c.0).unwrap_or(0) };
    let name = unsafe { bstr(elem.CurrentName()) };
    let aid = unsafe { bstr(elem.CurrentAutomationId()) };
    let enabled = unsafe { elem.CurrentIsEnabled().map(|b| b.as_bool()).unwrap_or(true) };
    let mut obj = serde_json::json!({
        "control_type": control_type_name(ct),
        "name": name,
        "enabled": enabled,
    });
    if !aid.is_empty() {
        obj["automation_id"] = serde_json::json!(aid);
    }
    if let Some((l, t, r, b)) = elem_rect(elem) {
        obj["bounds"] = serde_json::json!({ "x": l, "y": t, "width": r - l, "height": b - t });
        obj["center"] = serde_json::json!({ "x": (l + r) / 2, "y": (t + b) / 2 });
    }
    obj
}

/// Describe one cached element as JSON using only the properties already fetched
/// by BuildUpdatedCache. This avoids a cross-process call per property while
/// preserving the same output shape used by the live fallback.
fn describe_json_cached(elem: &IUIAutomationElement) -> serde_json::Value {
    let ct = unsafe { elem.CachedControlType().map(|c| c.0).unwrap_or(0) };
    let name = unsafe { bstr(elem.CachedName()) };
    let aid = unsafe { bstr(elem.CachedAutomationId()) };
    let enabled = unsafe { elem.CachedIsEnabled().map(|b| b.as_bool()).unwrap_or(true) };
    let mut obj = serde_json::json!({
        "control_type": control_type_name(ct),
        "name": name,
        "enabled": enabled,
    });
    if !aid.is_empty() {
        obj["automation_id"] = serde_json::json!(aid);
    }
    if let Some((l, t, r, b)) = cached_rect(elem) {
        obj["bounds"] = serde_json::json!({ "x": l, "y": t, "width": r - l, "height": b - t });
        obj["center"] = serde_json::json!({ "x": (l + r) / 2, "y": (t + b) / 2 });
    }
    obj
}

/// Search a window's tree for elements matching name and/or control-type
/// substrings (case-insensitive). Returns matches with their registry `index`
/// (valid for click_element until the next observe/get_ui_tree).
pub fn find_elements(
    hwnd: HWND,
    name: Option<&str>,
    control_type: Option<&str>,
    max_results: usize,
    max_elements: usize,
    max_depth: usize,
) -> Result<serde_json::Value, String> {
    let name = name.and_then(|s| {
        let trimmed = s.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    });
    let control_type = control_type.and_then(|s| {
        let trimmed = s.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    });
    let exact_control_type = control_type.and_then(control_type_id);
    if exact_control_type.is_none() {
        return find_elements_tree(
            hwnd,
            name,
            control_type,
            max_results,
            max_elements,
            max_depth,
        );
    }

    let auto = automation()?;
    let root = unsafe {
        auto.ElementFromHandle(hwnd)
            .map_err(|e| format!("ElementFromHandle failed: {e}"))?
    };
    if !not_null(&root) {
        return Err("window has no UI Automation element".into());
    }

    let cache = make_cache_request(&auto)?;
    let condition = if let Some(ct) = exact_control_type {
        unsafe {
            auto.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(ct))
                .map_err(|e| format!("CreatePropertyCondition(ControlType): {e}"))?
        }
    } else {
        unsafe {
            auto.ControlViewCondition()
                .map_err(|e| format!("ControlViewCondition: {e}"))?
        }
    };

    let found = unsafe {
        root.FindAllBuildCache(TreeScope_Subtree, &condition, &cache)
            .map_err(|e| format!("FindAllBuildCache failed: {e}"))?
    };
    let count = unsafe { found.Length().unwrap_or(0) };
    let name_l = name.map(|s| s.to_lowercase());
    let ct_l = control_type.map(|s| s.to_lowercase());
    let matches = REGISTRY.with(|r| {
        let mut reg = r.borrow_mut();
        reg.clear();
        let mut out = Vec::new();
        let scan_limit = (count as usize).min(max_elements);
        for i in 0..scan_limit {
            let Ok(elem) = (unsafe { found.GetElement(i as i32) }) else {
                continue;
            };
            let mut d = describe_json_cached(&elem);
            let cname = d["name"].as_str().unwrap_or("").to_lowercase();
            let cct = d["control_type"].as_str().unwrap_or("").to_lowercase();
            if let Some(n) = &name_l {
                if !cname.contains(n) {
                    continue;
                }
            }
            if let Some(t) = &ct_l {
                if !cct.contains(t) {
                    continue;
                }
            }
            let rect = cached_rect(&elem);
            let idx = reg.len();
            reg.push(Cached {
                elem: elem.clone(),
                rect,
            });
            d["index"] = serde_json::json!(idx);
            out.push(d);
            if out.len() >= max_results {
                break;
            }
        }
        out
    });
    Ok(serde_json::json!({ "count": matches.len(), "elements": matches }))
}

fn find_elements_tree(
    hwnd: HWND,
    name: Option<&str>,
    control_type: Option<&str>,
    max_results: usize,
    max_elements: usize,
    max_depth: usize,
) -> Result<serde_json::Value, String> {
    let _ = build_window_tree(hwnd, max_elements, max_depth, true)?;
    let name_l = name.map(|s| s.to_lowercase());
    let ct_l = control_type.map(|s| s.to_lowercase());
    let matches = REGISTRY.with(|r| {
        let reg = r.borrow();
        let mut out = Vec::new();
        for (i, c) in reg.iter().enumerate() {
            let mut d = describe_json_cached(&c.elem);
            let cname = d["name"].as_str().unwrap_or("").to_lowercase();
            let cct = d["control_type"].as_str().unwrap_or("").to_lowercase();
            if let Some(n) = &name_l {
                if !cname.contains(n) {
                    continue;
                }
            }
            if let Some(t) = &ct_l {
                if !cct.contains(t) {
                    continue;
                }
            }
            d["index"] = serde_json::json!(i);
            out.push(d);
            if out.len() >= max_results {
                break;
            }
        }
        out
    });
    Ok(serde_json::json!({ "count": matches.len(), "elements": matches }))
}

/// The element that currently has keyboard focus, as JSON.
pub fn focused_element() -> Result<serde_json::Value, String> {
    let auto = automation()?;
    let f = unsafe {
        auto.GetFocusedElement()
            .map_err(|e| format!("GetFocusedElement failed: {e}"))?
    };
    if !not_null(&f) {
        return Err("no focused element".into());
    }
    Ok(describe_json(&f))
}

/// The element at a screen point, as JSON.
pub fn element_at_point(x: i32, y: i32) -> Result<serde_json::Value, String> {
    let auto = automation()?;
    let pt = windows::Win32::Foundation::POINT { x, y };
    let e = unsafe {
        auto.ElementFromPoint(pt)
            .map_err(|e| format!("ElementFromPoint failed: {e}"))?
    };
    if !not_null(&e) {
        return Err("no element at point".into());
    }
    Ok(describe_json(&e))
}

fn with_element<F, R>(index: usize, f: F) -> Result<R, String>
where
    F: FnOnce(&IUIAutomationElement, Option<(i32, i32, i32, i32)>) -> Result<R, String>,
{
    REGISTRY.with(|r| {
        let reg = r.borrow();
        let cached = reg
            .get(index)
            .ok_or_else(|| format!("unknown element_index {index}; call get_window_state again"))?;
        f(&cached.elem, cached.rect)
    })
}

/// Screen-pixel center of a cached element.
pub fn element_center(index: usize) -> Result<(i32, i32), String> {
    with_element(index, |_e, rect| {
        let (l, t, r, b) = rect.ok_or("element has no on-screen bounds")?;
        Ok(((l + r) / 2, (t + b) / 2))
    })
}

/// Bounding boxes (index, left, top, width, height) in screen pixels for every
/// element in the current registry that has on-screen bounds. Used to draw the
/// Set-of-Marks overlay. Must be called on the worker thread that built the tree.
pub fn registry_marks() -> Vec<(i64, i32, i32, i32, i32)> {
    REGISTRY.with(|r| {
        r.borrow()
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.rect.map(|(l, t, rr, b)| (i as i64, l, t, rr - l, b - t)))
            .collect()
    })
}

pub fn focus_element(index: usize) {
    let _ = with_element(index, |e, _r| {
        unsafe {
            let _ = e.SetFocus();
        }
        Ok(())
    });
}

/// Set an editable element's value via the ValuePattern, focusing it first.
pub fn set_value(index: usize, value: &str) -> Result<(), String> {
    with_element(index, |e, _r| unsafe {
        let _ = e.SetFocus();
        if let Ok(pat) = e.GetCurrentPattern(UIA_ValuePatternId) {
            if not_null(&pat) {
                let vp: IUIAutomationValuePattern = pat
                    .cast()
                    .map_err(|err| format!("ValuePattern cast failed: {err}"))?;
                vp.SetValue(&BSTR::from(value))
                    .map_err(|err| format!("SetValue failed: {err}"))?;
                return Ok(());
            }
        }
        let numeric = value.trim().parse::<f64>().map_err(|_| {
            "element does not support ValuePattern; RangeValue needs a number".to_string()
        })?;
        let pat = e
            .GetCurrentPattern(UIA_RangeValuePatternId)
            .map_err(|err| format!("no RangeValuePattern: {err}"))?;
        if !not_null(&pat) {
            return Err("element supports neither ValuePattern nor RangeValuePattern".into());
        }
        let rp: IUIAutomationRangeValuePattern = pat.cast().map_err(|e| format!("{e}"))?;
        rp.SetValue(numeric)
            .map_err(|e| format!("RangeValue SetValue failed: {e}"))
    })
}

/// Invoke an element's default action through the InvokePattern.
pub fn invoke(index: usize) -> Result<(), String> {
    with_element(index, |e, _r| unsafe {
        let pat = e
            .GetCurrentPattern(UIA_InvokePatternId)
            .map_err(|err| format!("no InvokePattern: {err}"))?;
        if !not_null(&pat) {
            return Err("element does not support InvokePattern".into());
        }
        let ip: IUIAutomationInvokePattern = pat.cast().map_err(|e| format!("{e}"))?;
        ip.Invoke().map_err(|e| format!("Invoke failed: {e}"))?;
        Ok(())
    })
}

/// Perform a secondary accessibility action: Raise / Expand / Collapse / Scroll *.
pub fn perform_secondary(index: usize, action: &str) -> Result<(), String> {
    let act = action.trim().to_ascii_lowercase();
    with_element(index, |e, _r| unsafe {
        match act.as_str() {
            "raise" => {
                let _ = e.SetFocus();
                Ok(())
            }
            "expand" | "collapse" => {
                let pat = e
                    .GetCurrentPattern(UIA_ExpandCollapsePatternId)
                    .map_err(|err| format!("no ExpandCollapsePattern: {err}"))?;
                if !not_null(&pat) {
                    return Err("element does not support ExpandCollapse".into());
                }
                let ec: IUIAutomationExpandCollapsePattern =
                    pat.cast().map_err(|e| format!("{e}"))?;
                if act == "expand" {
                    ec.Expand().map_err(|e| format!("Expand failed: {e}"))
                } else {
                    ec.Collapse().map_err(|e| format!("Collapse failed: {e}"))
                }
            }
            "scroll up" | "scroll down" | "scroll left" | "scroll right" => {
                let pat = e
                    .GetCurrentPattern(UIA_ScrollPatternId)
                    .map_err(|err| format!("no ScrollPattern: {err}"))?;
                if !not_null(&pat) {
                    return Err("element does not support Scroll".into());
                }
                let sp: IUIAutomationScrollPattern = pat.cast().map_err(|e| format!("{e}"))?;
                let (h, v) = match act.as_str() {
                    "scroll up" => (ScrollAmount_NoAmount, ScrollAmount_LargeDecrement),
                    "scroll down" => (ScrollAmount_NoAmount, ScrollAmount_LargeIncrement),
                    "scroll left" => (ScrollAmount_LargeDecrement, ScrollAmount_NoAmount),
                    _ => (ScrollAmount_LargeIncrement, ScrollAmount_NoAmount),
                };
                sp.Scroll(h, v).map_err(|e| format!("Scroll failed: {e}"))
            }
            "scroll into view" | "scrollintoview" | "scroll_into_view" => {
                let pat = e
                    .GetCurrentPattern(UIA_ScrollItemPatternId)
                    .map_err(|err| format!("no ScrollItemPattern: {err}"))?;
                if !not_null(&pat) {
                    return Err("element does not support ScrollItem".into());
                }
                let sp: IUIAutomationScrollItemPattern = pat.cast().map_err(|e| format!("{e}"))?;
                sp.ScrollIntoView()
                    .map_err(|e| format!("ScrollIntoView failed: {e}"))
            }
            "realize" | "virtualize" | "virtualized" => {
                let pat = e
                    .GetCurrentPattern(UIA_VirtualizedItemPatternId)
                    .map_err(|err| format!("no VirtualizedItemPattern: {err}"))?;
                if !not_null(&pat) {
                    return Err("element does not support VirtualizedItem".into());
                }
                let vp: IUIAutomationVirtualizedItemPattern =
                    pat.cast().map_err(|e| format!("{e}"))?;
                vp.Realize().map_err(|e| format!("Realize failed: {e}"))
            }
            _ if act.starts_with("range ") || act.starts_with("set range ") => {
                let value = act
                    .split_whitespace()
                    .last()
                    .ok_or("missing range value")?
                    .parse::<f64>()
                    .map_err(|_| format!("invalid range value in action: {action}"))?;
                let pat = e
                    .GetCurrentPattern(UIA_RangeValuePatternId)
                    .map_err(|err| format!("no RangeValuePattern: {err}"))?;
                if !not_null(&pat) {
                    return Err("element does not support RangeValue".into());
                }
                let rp: IUIAutomationRangeValuePattern = pat.cast().map_err(|e| format!("{e}"))?;
                rp.SetValue(value)
                    .map_err(|e| format!("RangeValue SetValue failed: {e}"))
            }
            "toggle" => {
                let pat = e
                    .GetCurrentPattern(UIA_TogglePatternId)
                    .map_err(|err| format!("no TogglePattern: {err}"))?;
                if !not_null(&pat) {
                    return Err("element does not support Toggle".into());
                }
                let tp: IUIAutomationTogglePattern = pat.cast().map_err(|e| format!("{e}"))?;
                tp.Toggle().map_err(|e| format!("Toggle failed: {e}"))
            }
            "select" => {
                let pat = e
                    .GetCurrentPattern(UIA_SelectionItemPatternId)
                    .map_err(|err| format!("no SelectionItemPattern: {err}"))?;
                if !not_null(&pat) {
                    return Err("element does not support SelectionItem".into());
                }
                let si: IUIAutomationSelectionItemPattern =
                    pat.cast().map_err(|e| format!("{e}"))?;
                si.Select().map_err(|e| format!("Select failed: {e}"))
            }
            _ => Err(format!("unsupported secondary action: {action}")),
        }
    })
}

/// Silence unused-import warnings for collapse-state constant in some builds.
#[allow(dead_code)]
fn _touch() {
    let _ = ExpandCollapseState_Collapsed;
}
