//! Color themes.
//!
//! A [`Theme`] is a set of semantic roles (background, text, accent, ok /
//! warn / danger, gradients) loaded from TOML. The [`Catalog`] holds the
//! themes embedded in the binary plus the user's own files in
//! `~/.config/apptop/themes`. No color is written anywhere else in apptop:
//! the rest of the code asks the theme for a role.
//!
//! - [`theme`]: the model, TOML loading and validation, style helpers.
//! - [`catalog`]: embedded themes and the user's directory.
//! - [`color`]: color-depth detection, 256 and 16 color fallback, contrast.
//! - [`picker`]: the theme picker (key `T`) with live preview.
//! - [`persist`]: saving the chosen theme into the config file.
//!
//! `docs/THEMES.md` explains how to write a theme.

pub mod catalog;
pub mod color;
pub mod persist;
pub mod picker;
pub mod theme;

pub use catalog::{user_dir, Catalog, USER_FAMILY};
pub use color::{ColorDepth, Rgb};
pub use persist::save_theme;
pub use picker::{Labels, Picker, PickerEvent, PickerKey};
pub use theme::{Appearance, IconMode, Origin, Theme, ThemeError};
