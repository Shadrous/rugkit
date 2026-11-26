//! Accessibility tree and action types, bridging to AccessKit internally.
//!
//! No `accesskit` types appear in rugkit's public API. Both VoiceOver (via AccessKit) and
//! rugkit's headless test harness operate on [`A11yTree`] and [`A11yAction`].

use rugkit_render::Rect;

/// Identifies a node in an [`A11yTree`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct A11yId(pub u64);

/// The semantic role of an accessibility node.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// Top-level window.
    Window,
    /// A generic grouping container.
    Group,
    /// Static or dynamic read-only text.
    Label,
    /// A clickable button.
    Button,
    /// An editable text field.
    TextInput,
    /// A checkbox.
    Checkbox,
    /// A toggle switch.
    Switch,
    /// A range slider.
    Slider,
    /// A radio button.
    RadioButton,
    /// A dropdown / select control.
    ComboBox,
    /// A tab button inside a tab list.
    Tab,
    /// A container of tabs.
    TabList,
    /// A modal dialog.
    Dialog,
    /// A list container.
    List,
    /// An item inside a list.
    ListItem,
    /// A scrollable viewport.
    ScrollView,
    /// An image or icon.
    Image,
    /// A tooltip popup.
    Tooltip,
}

/// One node in the accessibility tree.
#[derive(Clone, Debug, PartialEq)]
pub struct A11yNode {
    /// Unique node identifier within the window.
    pub id: A11yId,
    /// Semantic role.
    pub role: Role,
    /// Accessible name / label read by screen readers.
    pub label: Option<String>,
    /// Current value (e.g. text inside a [`Role::TextInput`]).
    pub value: Option<String>,
    /// Checked / toggled state for checkboxes, switches and radio buttons.
    pub checked: Option<bool>,
    /// Numeric value for sliders.
    pub numeric_value: Option<f64>,
    /// Minimum and maximum numeric bounds for sliders.
    pub numeric_range: Option<(f64, f64)>,
    /// Bounding rectangle in logical window coordinates.
    pub bounds: Rect,
    /// Child node IDs in traversal order.
    pub children: Vec<A11yId>,
    /// Whether this node can receive keyboard focus.
    pub focusable: bool,
    /// Whether this node is disabled.
    pub disabled: bool,
}

impl A11yNode {
    /// Creates a new accessibility node with the given ID, role and bounds.
    pub fn new(id: A11yId, role: Role, bounds: Rect) -> Self {
        Self {
            id,
            role,
            label: None,
            value: None,
            checked: None,
            numeric_value: None,
            numeric_range: None,
            bounds,
            children: Vec::new(),
            focusable: false,
            disabled: false,
        }
    }
}

/// Snapshot of a window's accessibility tree.
#[derive(Clone, Debug, PartialEq)]
pub struct A11yTree {
    /// Root node (typically [`Role::Window`]).
    pub root: A11yId,
    /// Currently focused node (or `root` if no child has focus).
    pub focus: A11yId,
    /// Every node in the tree, including `root`.
    pub nodes: Vec<A11yNode>,
}

/// An accessibility action requested by a screen reader (e.g. VoiceOver) or test harness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum A11yAction {
    /// Activate / click the target node.
    Click(A11yId),
    /// Move keyboard focus to the target node.
    Focus(A11yId),
    /// Remove keyboard focus from the target node.
    Blur(A11yId),
    /// Replace the target node's value (e.g. setting text in a [`Role::TextInput`]).
    SetValue(A11yId, String),
    /// Replace the currently selected text in a [`Role::TextInput`].
    ReplaceSelectedText(A11yId, String),
}

// ---- AccessKit conversion (crate-private) -----------------------------------------------

impl A11yTree {
    pub(crate) fn to_accesskit(&self, scale: f64) -> accesskit::TreeUpdate {
        let nodes = self
            .nodes
            .iter()
            .map(|n| (accesskit::NodeId(n.id.0), node_to_accesskit(n, scale)))
            .collect();
        let mut tree = accesskit::TreeInfo::new(accesskit::NodeId(self.root.0));
        tree.toolkit_name = Some("rugkit".into());
        accesskit::TreeUpdate {
            nodes,
            tree: Some(tree),
            tree_id: accesskit::TreeId::ROOT,
            focus: accesskit::NodeId(self.focus.0),
        }
    }
}

fn node_to_accesskit(n: &A11yNode, scale: f64) -> accesskit::Node {
    let role = match n.role {
        Role::Window => accesskit::Role::Window,
        Role::Group => accesskit::Role::GenericContainer,
        Role::Label => accesskit::Role::Label,
        Role::Button => accesskit::Role::Button,
        Role::TextInput => accesskit::Role::TextInput,
        Role::Checkbox => accesskit::Role::CheckBox,
        Role::Switch => accesskit::Role::Switch,
        Role::Slider => accesskit::Role::Slider,
        Role::RadioButton => accesskit::Role::RadioButton,
        Role::ComboBox => accesskit::Role::ComboBox,
        Role::Tab => accesskit::Role::Tab,
        Role::TabList => accesskit::Role::TabList,
        Role::Dialog => accesskit::Role::Dialog,
        Role::List => accesskit::Role::List,
        Role::ListItem => accesskit::Role::ListItem,
        Role::ScrollView => accesskit::Role::ScrollView,
        Role::Image => accesskit::Role::Image,
        Role::Tooltip => accesskit::Role::Tooltip,
    };
    let mut node = accesskit::Node::new(role);
    if let Some(label) = &n.label {
        node.set_label(label.as_str());
    }
    if let Some(value) = &n.value {
        node.set_value(value.as_str());
    }
    if let Some(checked) = n.checked {
        node.set_toggled(if checked {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
        });
    }
    if let Some(val) = n.numeric_value {
        node.set_numeric_value(val);
    }
    if let Some((min, max)) = n.numeric_range {
        node.set_min_numeric_value(min);
        node.set_max_numeric_value(max);
    }
    node.set_bounds(accesskit::Rect {
        x0: n.bounds.x0 * scale,
        y0: n.bounds.y0 * scale,
        x1: n.bounds.x1 * scale,
        y1: n.bounds.y1 * scale,
    });
    if !n.children.is_empty() {
        node.set_children(
            n.children
                .iter()
                .map(|c| accesskit::NodeId(c.0))
                .collect::<Vec<_>>(),
        );
    }
    if n.focusable {
        node.add_action(accesskit::Action::Focus);
        node.add_action(accesskit::Action::Blur);
    }
    if n.disabled {
        node.set_disabled();
    }
    match n.role {
        Role::Button
        | Role::Checkbox
        | Role::Switch
        | Role::RadioButton
        | Role::ComboBox
        | Role::Tab => {
            node.add_action(accesskit::Action::Click);
        }
        Role::TextInput => {
            node.add_action(accesskit::Action::Click);
            node.add_action(accesskit::Action::SetValue);
            node.add_action(accesskit::Action::ReplaceSelectedText);
        }
        _ => {}
    }
    node
}

pub(crate) fn action_from_accesskit(req: accesskit::ActionRequest) -> Option<A11yAction> {
    let id = A11yId(req.target_node.0);
    match req.action {
        accesskit::Action::Click => Some(A11yAction::Click(id)),
        accesskit::Action::Focus => Some(A11yAction::Focus(id)),
        accesskit::Action::Blur => Some(A11yAction::Blur(id)),
        accesskit::Action::SetValue => match req.data {
            Some(accesskit::ActionData::Value(v)) => Some(A11yAction::SetValue(id, v.into())),
            _ => None,
        },
        accesskit::Action::ReplaceSelectedText => match req.data {
            Some(accesskit::ActionData::Value(v)) => {
                Some(A11yAction::ReplaceSelectedText(id, v.into()))
            }
            _ => None,
        },
        _ => None,
    }
}
