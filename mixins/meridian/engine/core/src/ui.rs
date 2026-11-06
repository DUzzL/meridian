//! A small widget set for Meridian's screens, in the look of the game's
//! own: buttons from the game's button sprites, black text fields with a
//! grey edge, compact selectors, labels.
//!
//! Screens lay their widgets out each frame ([`Widget`] list); the same
//! list answers clicks ([`hit`]) and draws ([`draw`]).

use crate::minimap::{Align, DrawOp};

/// Height of a button or text field, GUI pixels.
pub const BUTTON_H: f32 = 20.0;
/// Height of a selector, GUI pixels.
pub const SELECTOR_H: f32 = 11.0;

const WHITE: u32 = 0xFFFF_FFFF;
const DISABLED_TEXT: u32 = 0xFFA0_A0A0;
const PLACEHOLDER: u32 = 0xFF70_7070;
const EDGE: u32 = 0xFFA0_A0A0;
const EDGE_FOCUSED: u32 = 0xFFFF_FFFF;

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Button {
        enabled: bool,
    },
    /// A text field; `text` is its content.
    Field {
        placeholder: String,
        focused: bool,
        color: u32,
    },
    /// A value that changes on click (left: next, right: previous).
    Selector {
        color: u32,
    },
    /// Plain text; not clickable.
    Label {
        color: u32,
        align: Align,
    },
    /// A clickable list row, highlighted when selected.
    Row {
        selected: bool,
    },
    /// A thin horizontal line across `rect`.
    Separator,
    /// A waypoint's mark: a square in `color` with the text (its initial)
    /// in white.
    Mark {
        color: u32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Widget {
    pub id: String,
    pub rect: [f32; 4],
    pub text: String,
    pub kind: Kind,
}

impl Widget {
    pub fn button(id: &str, rect: [f32; 4], text: impl Into<String>, enabled: bool) -> Self {
        Self {
            id: id.into(),
            rect,
            text: text.into(),
            kind: Kind::Button { enabled },
        }
    }

    pub fn field(id: &str, rect: [f32; 4], text: &str, placeholder: &str, focused: bool) -> Self {
        Self {
            id: id.into(),
            rect,
            text: text.into(),
            kind: Kind::Field {
                placeholder: placeholder.into(),
                focused,
                color: 0xFFE0_E0E0,
            },
        }
    }

    pub fn selector(id: &str, rect: [f32; 4], text: impl Into<String>, color: u32) -> Self {
        Self {
            id: id.into(),
            rect,
            text: text.into(),
            kind: Kind::Selector { color },
        }
    }

    pub fn label(x: f32, y: f32, text: impl Into<String>, align: Align) -> Self {
        Self {
            id: String::new(),
            rect: [x, y, 0.0, 9.0],
            text: text.into(),
            kind: Kind::Label {
                color: WHITE,
                align,
            },
        }
    }

    pub fn row(id: &str, rect: [f32; 4], selected: bool) -> Self {
        Self {
            id: id.into(),
            rect,
            text: String::new(),
            kind: Kind::Row { selected },
        }
    }

    pub fn separator(y: f32, width: f32) -> Self {
        Self {
            id: String::new(),
            rect: [0.0, y, width, 2.0],
            text: String::new(),
            kind: Kind::Separator,
        }
    }

    fn contains(&self, p: [f32; 2]) -> bool {
        let [x, y, w, h] = self.rect;
        p[0] >= x && p[0] < x + w && p[1] >= y && p[1] < y + h
    }

    fn interactive(&self) -> bool {
        match &self.kind {
            Kind::Button { enabled } => *enabled,
            Kind::Field { .. } | Kind::Selector { .. } | Kind::Row { .. } => true,
            Kind::Label { .. } | Kind::Separator | Kind::Mark { .. } => false,
        }
    }
}

/// The widget under `p` that takes clicks, last drawn first.
pub fn hit(widgets: &[Widget], p: [f32; 2]) -> Option<&Widget> {
    widgets
        .iter()
        .rev()
        .find(|w| w.interactive() && w.contains(p))
}

/// Draws widgets; `mouse` highlights what it is over.
pub fn draw(widgets: &[Widget], mouse: [f32; 2]) -> Vec<DrawOp> {
    let mut ops = Vec::new();
    for w in widgets {
        let [x, y, width, height] = w.rect;
        let hover = w.interactive() && w.contains(mouse);
        let center_text = |ops: &mut Vec<DrawOp>, color: u32| {
            ops.push(DrawOp::Text {
                x: x + width / 2.0,
                y: y + (height - 8.0) / 2.0,
                text: w.text.clone(),
                argb: color,
                align: Align::Center,
                scale: 1.0,
            });
        };
        match &w.kind {
            Kind::Button { enabled } => {
                let sprite = match (enabled, hover) {
                    (false, _) => "gui/sprites/widget/button_disabled",
                    (true, true) => "gui/sprites/widget/button_highlighted",
                    (true, false) => "gui/sprites/widget/button",
                };
                ops.push(DrawOp::NineSlice {
                    sprite,
                    rect: w.rect,
                    argb: WHITE,
                });
                center_text(&mut ops, if *enabled { WHITE } else { DISABLED_TEXT });
            }
            Kind::Field {
                placeholder,
                focused,
                color,
            } => {
                ops.push(DrawOp::Fill {
                    rect: w.rect,
                    argb: if *focused { EDGE_FOCUSED } else { EDGE },
                });
                ops.push(DrawOp::Fill {
                    rect: [x + 1.0, y + 1.0, width - 2.0, height - 2.0],
                    argb: 0xFF00_0000,
                });
                let (text, color) = if w.text.is_empty() && !*focused {
                    (placeholder.clone(), PLACEHOLDER)
                } else if *focused {
                    (format!("{}_", w.text), *color)
                } else {
                    (w.text.clone(), *color)
                };
                ops.push(DrawOp::Text {
                    x: x + 4.0,
                    y: y + (height - 8.0) / 2.0,
                    text,
                    argb: color,
                    align: Align::Left,
                    scale: 1.0,
                });
            }
            Kind::Selector { color } => {
                ops.push(DrawOp::Fill {
                    rect: w.rect,
                    argb: if hover { EDGE_FOCUSED } else { EDGE },
                });
                ops.push(DrawOp::Fill {
                    rect: [x + 1.0, y + 1.0, width - 2.0, height - 2.0],
                    argb: 0xFF00_0000,
                });
                ops.push(DrawOp::Text {
                    x: x + width / 2.0,
                    y: y + (height - 8.0) / 2.0 + 0.5,
                    text: w.text.clone(),
                    argb: *color,
                    align: Align::Center,
                    scale: 1.0,
                });
            }
            Kind::Label { color, align } => {
                ops.push(DrawOp::Text {
                    x,
                    y,
                    text: w.text.clone(),
                    argb: *color,
                    align: *align,
                    scale: 1.0,
                });
            }
            Kind::Row { selected } => {
                if *selected {
                    ops.push(DrawOp::Fill {
                        rect: w.rect,
                        argb: 0xFF80_8080,
                    });
                    ops.push(DrawOp::Fill {
                        rect: [x + 1.0, y + 1.0, width - 2.0, height - 2.0],
                        argb: 0xFF00_0000,
                    });
                } else if hover {
                    ops.push(DrawOp::Fill {
                        rect: w.rect,
                        argb: 0x30FF_FFFF,
                    });
                }
            }
            Kind::Mark { color } => {
                ops.push(DrawOp::Fill {
                    rect: w.rect,
                    argb: *color,
                });
                ops.push(DrawOp::Text {
                    x: x + width / 2.0 + 0.5,
                    y: y + (height - 8.0) / 2.0 + 0.5,
                    text: w.text.clone(),
                    argb: WHITE,
                    align: Align::Center,
                    scale: 1.0,
                });
            }
            Kind::Separator => {
                ops.push(DrawOp::Fill {
                    rect: [x, y, width, 1.0],
                    argb: 0xFF00_0000,
                });
                ops.push(DrawOp::Fill {
                    rect: [x, y + 1.0, width, 1.0],
                    argb: 0x60FF_FFFF,
                });
            }
        }
    }
    ops
}

/// What typing does to a text field: a character is appended (when it
/// fits `accept`), backspace removes the last one.
pub fn type_char(text: &mut String, c: char, max: usize, accept: impl Fn(char) -> bool) -> bool {
    if c.is_control() || !accept(c) || text.chars().count() >= max {
        return false;
    }
    text.push(c);
    true
}

pub fn backspace(text: &mut String) -> bool {
    text.pop().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_skips_disabled_and_labels() {
        let ws = vec![
            Widget::label(0.0, 0.0, "x", Align::Left),
            Widget::button("a", [0.0, 0.0, 50.0, 20.0], "A", true),
            Widget::button("b", [0.0, 0.0, 50.0, 20.0], "B", false),
        ];
        assert_eq!(hit(&ws, [10.0, 10.0]).map(|w| w.id.as_str()), Some("a"));
        assert!(hit(&ws, [60.0, 10.0]).is_none());
    }

    #[test]
    fn buttons_use_the_games_sprites() {
        let ws = vec![
            Widget::button("a", [0.0, 0.0, 50.0, 20.0], "A", true),
            Widget::button("b", [0.0, 30.0, 50.0, 20.0], "B", false),
        ];
        let ops = draw(&ws, [5.0, 5.0]);
        let sprites: Vec<_> = ops
            .iter()
            .filter_map(|o| match o {
                DrawOp::NineSlice { sprite, .. } => Some(*sprite),
                _ => None,
            })
            .collect();
        assert_eq!(
            sprites,
            [
                "gui/sprites/widget/button_highlighted",
                "gui/sprites/widget/button_disabled"
            ]
        );
    }

    #[test]
    fn fields_show_placeholder_or_cursor() {
        let idle = draw(
            &[Widget::field(
                "n",
                [0.0, 0.0, 100.0, 20.0],
                "",
                "name",
                false,
            )],
            [500.0, 500.0],
        );
        assert!(
            idle.iter().any(
                |o| matches!(o, DrawOp::Text { text, argb: PLACEHOLDER, .. } if text == "name")
            )
        );
        let typing = draw(
            &[Widget::field(
                "n",
                [0.0, 0.0, 100.0, 20.0],
                "Ho",
                "name",
                true,
            )],
            [500.0, 500.0],
        );
        assert!(
            typing
                .iter()
                .any(|o| matches!(o, DrawOp::Text { text, .. } if text == "Ho_"))
        );
    }

    #[test]
    fn typing_rules() {
        let mut t = String::new();
        let digits = |c: char| c.is_ascii_digit() || c == '-';
        assert!(type_char(&mut t, '1', 3, digits));
        assert!(!type_char(&mut t, 'a', 3, digits));
        assert!(type_char(&mut t, '2', 3, digits));
        assert!(type_char(&mut t, '3', 3, digits));
        assert!(!type_char(&mut t, '4', 3, digits), "full");
        assert!(!type_char(&mut t, '\n', 9, |_| true));
        assert!(backspace(&mut t));
        assert_eq!(t, "12");
    }
}
