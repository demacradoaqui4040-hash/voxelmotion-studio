// VoxelMotion Studio - Data-Driven DesignSpec Engine (5 Hero Artistic Directions)
// Drives geometry, border polarity, switch mechanics, typography hierarchy, and theme contrast from Rust.

use crate::Theme;
use slint::Color;

#[derive(Clone, Copy, Debug)]
pub struct DesignSpec {
    pub idx: i32,
    pub name: &'static str,
    pub radius_control: f32,
    pub radius_overlay: f32,
    pub border_control_w: f32,
    pub topbar_divider: bool,
    pub switch_pill: bool,
    pub wireframe_controls: bool,
    pub mono_headers: bool,

    pub bg_shell: Color,
    pub bg_sidebar: Color,
    pub bg_elevated: Color,
    pub bg_input: Color,
    pub bg_hover: Color,
    pub bg_active: Color,
    pub bg_popover: Color,
    pub bg_toast: Color,
    pub bg_toggle_off: Color,
    pub bg_ruler: Color,
    pub bg_guide: Color,

    pub divider: Color,
    pub border_control: Color,
    pub border_specular: Color,

    pub accent: Color,
    pub accent_hover: Color,
    pub accent_press: Color,
    pub accent_soft: Color,
    pub accent_text: Color,

    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_dim: Color,

    pub axis_x: Color,
    pub axis_y: Color,
    pub axis_z: Color,
    pub gold: Color,
    pub green: Color,
}

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::from_argb_u8(a, r, g, b)
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb_u8(r, g, b)
}

pub fn spec_for_theme(idx: i32) -> DesignSpec {
    match idx.clamp(0, 4) {
        // 1. Field Hardware: Matte industrial aluminum, 2px sharp corners, mechanical toggles, JetBrains Mono headers
        1 => DesignSpec {
            idx: 1,
            name: "Field Hardware",
            radius_control: 2.0,
            radius_overlay: 3.0,
            border_control_w: 1.0,
            topbar_divider: true,
            switch_pill: false,
            wireframe_controls: false,
            mono_headers: true,

            bg_shell: rgb(0x14, 0x15, 0x17),
            bg_sidebar: rgb(0x1b, 0x1c, 0x20),
            bg_elevated: rgb(0x25, 0x27, 0x2c),
            bg_input: rgb(0x0f, 0x10, 0x12),
            bg_hover: rgb(0x2c, 0x2f, 0x36),
            bg_active: rgb(0x34, 0x38, 0x41),
            bg_popover: rgba(0x1d, 0x1f, 0x23, 0xf6),
            bg_toast: rgba(0x1b, 0x1c, 0x20, 0xf6),
            bg_toggle_off: rgb(0x31, 0x34, 0x3d),
            bg_ruler: rgb(0x17, 0x18, 0x1c),
            bg_guide: rgb(0x28, 0x2b, 0x32),

            divider: rgb(0x32, 0x35, 0x3e),
            border_control: rgb(0x41, 0x45, 0x52),
            border_specular: rgb(0x52, 0x57, 0x66),

            accent: rgb(0xff, 0x55, 0x00),
            accent_hover: rgb(0xff, 0x6d, 0x24),
            accent_press: rgb(0xd9, 0x46, 0x00),
            accent_soft: rgba(0xff, 0x55, 0x00, 0x2b),
            accent_text: rgb(0xff, 0xb8, 0x8a),

            text_primary: rgb(0xf2, 0xf3, 0xf5),
            text_secondary: rgb(0xa3, 0xa8, 0xb4),
            text_dim: rgb(0x6d, 0x72, 0x80),

            axis_x: rgb(0xf4, 0x3f, 0x5e),
            axis_y: rgb(0x22, 0xc5, 0x5e),
            axis_z: rgb(0x3b, 0x82, 0xf6),
            gold: rgb(0xf5, 0x9e, 0x0b),
            green: rgb(0x22, 0xc5, 0x5e),
        },

        // 2. Daylight Studio: Light porcelain chrome + calibrated mid-grey 3D stage (keeps VFX readable)
        2 => DesignSpec {
            idx: 2,
            name: "Daylight Studio",
            radius_control: 7.0,
            radius_overlay: 10.0,
            border_control_w: 1.0,
            topbar_divider: true,
            switch_pill: true,
            wireframe_controls: false,
            mono_headers: false,

            bg_shell: rgb(0xe7, 0xea, 0xef),
            bg_sidebar: rgb(0xf4, 0xf5, 0xf8),
            bg_elevated: rgb(0xff, 0xff, 0xff),
            bg_input: rgb(0xe2, 0xe5, 0xec),
            bg_hover: rgba(0x00, 0x00, 0x00, 0x0d),
            bg_active: rgba(0x00, 0x00, 0x00, 0x16),
            bg_popover: rgba(0xff, 0xff, 0xff, 0xe0),
            bg_toast: rgba(0x1f, 0x23, 0x2d, 0xf2),
            bg_toggle_off: rgb(0xc8, 0xce, 0xda),
            bg_ruler: rgb(0xec, 0xee, 0xf3),
            bg_guide: rgb(0xd8, 0xdd, 0xe6),

            divider: rgb(0xd0, 0xd5, 0xdf),
            border_control: rgb(0xc5, 0xcc, 0xd8),
            border_specular: rgb(0xb8, 0xc0, 0xce),

            accent: rgb(0x00, 0x66, 0xcc),
            accent_hover: rgb(0x10, 0x76, 0xdb),
            accent_press: rgb(0x00, 0x52, 0xa8),
            accent_soft: rgba(0x00, 0x66, 0xcc, 0x1f),
            accent_text: rgb(0x00, 0x52, 0xa8),

            text_primary: rgb(0x14, 0x17, 0x1f),
            text_secondary: rgb(0x4b, 0x52, 0x62),
            text_dim: rgb(0x7c, 0x84, 0x95),

            axis_x: rgb(0xdc, 0x26, 0x26),
            axis_y: rgb(0x16, 0xa3, 0x4a),
            axis_z: rgb(0x00, 0x66, 0xcc),
            gold: rgb(0xd9, 0x77, 0x06),
            green: rgb(0x16, 0xa3, 0x4a),
        },

        // 3. CAD Blueprint: Zero radius orthogonal wireframes, cobalt drafting board, JetBrains Mono
        3 => DesignSpec {
            idx: 3,
            name: "CAD Blueprint",
            radius_control: 0.0,
            radius_overlay: 0.0,
            border_control_w: 1.0,
            topbar_divider: true,
            switch_pill: false,
            wireframe_controls: true,
            mono_headers: true,

            bg_shell: rgb(0x08, 0x11, 0x1f),
            bg_sidebar: rgb(0x0b, 0x17, 0x29),
            bg_elevated: rgb(0x11, 0x22, 0x3b),
            bg_input: rgb(0x06, 0x0d, 0x18),
            bg_hover: rgba(0x38, 0xbd, 0xf8, 0x1a),
            bg_active: rgba(0x38, 0xbd, 0xf8, 0x2b),
            bg_popover: rgba(0x0d, 0x1b, 0x31, 0xf5),
            bg_toast: rgba(0x0d, 0x1b, 0x31, 0xf5),
            bg_toggle_off: rgb(0x17, 0x2e, 0x52),
            bg_ruler: rgb(0x0a, 0x15, 0x26),
            bg_guide: rgb(0x16, 0x2c, 0x4e),

            divider: rgb(0x1d, 0x3c, 0x6a),
            border_control: rgb(0x2e, 0x5d, 0x9f),
            border_specular: rgb(0x38, 0xbd, 0xf8),

            accent: rgb(0x02, 0x84, 0xc7),
            accent_hover: rgb(0x38, 0xbd, 0xf8),
            accent_press: rgb(0x03, 0x69, 0xa1),
            accent_soft: rgba(0x38, 0xbd, 0xf8, 0x28),
            accent_text: rgb(0xba, 0xe6, 0xfd),

            text_primary: rgb(0xe6, 0xf2, 0xff),
            text_secondary: rgb(0x8d, 0xb5, 0xe0),
            text_dim: rgb(0x52, 0x76, 0xa3),

            axis_x: rgb(0xf4, 0x3f, 0x5e),
            axis_y: rgb(0x34, 0xd3, 0x99),
            axis_z: rgb(0x38, 0xbd, 0xf8),
            gold: rgb(0xfb, 0xbf, 0x24),
            green: rgb(0x34, 0xd3, 0x99),
        },

        // 4. Studio Clay: Warm sculpt dark stone + terracotta accent, soft editorial radii
        4 => DesignSpec {
            idx: 4,
            name: "Studio Clay",
            radius_control: 8.0,
            radius_overlay: 12.0,
            border_control_w: 1.0,
            topbar_divider: false,
            switch_pill: true,
            wireframe_controls: false,
            mono_headers: false,

            bg_shell: rgb(0x15, 0x13, 0x11),
            bg_sidebar: rgb(0x1d, 0x1a, 0x17),
            bg_elevated: rgb(0x28, 0x24, 0x20),
            bg_input: rgb(0x11, 0x0f, 0x0d),
            bg_hover: rgba(0xff, 0xf5, 0xeb, 0x10),
            bg_active: rgba(0xff, 0xf5, 0xeb, 0x1d),
            bg_popover: rgba(0x23, 0x1f, 0x1c, 0xf4),
            bg_toast: rgba(0x23, 0x1f, 0x1c, 0xf4),
            bg_toggle_off: rgb(0x36, 0x30, 0x2b),
            bg_ruler: rgb(0x1a, 0x17, 0x14),
            bg_guide: rgb(0x2b, 0x26, 0x21),

            divider: rgba(0xff, 0xf5, 0xeb, 0x14),
            border_control: rgba(0xff, 0xf5, 0xeb, 0x1a),
            border_specular: rgba(0xff, 0xf5, 0xeb, 0x26),

            accent: rgb(0xe0, 0x7a, 0x38),
            accent_hover: rgb(0xee, 0x8b, 0x4a),
            accent_press: rgb(0xc4, 0x62, 0x24),
            accent_soft: rgba(0xe0, 0x7a, 0x38, 0x29),
            accent_text: rgb(0xfd, 0xba, 0x74),

            text_primary: rgb(0xf5, 0xf1, 0xec),
            text_secondary: rgb(0xaa, 0xa0, 0x95),
            text_dim: rgb(0x74, 0x6b, 0x62),

            axis_x: rgb(0xff, 0x57, 0x57),
            axis_y: rgb(0x3d, 0xd6, 0x77),
            axis_z: rgb(0x4d, 0x9c, 0xff),
            gold: rgb(0xf5, 0xb8, 0x41),
            green: rgb(0x3d, 0xd6, 0x77),
        },

        // 0 (Default). Pro Studio Neutral: Calibrated neutral graphite, seamless 36px Silent Chrome, crisp hairlines
        _ => DesignSpec {
            idx: 0,
            name: "Pro Studio Neutral",
            radius_control: 6.0,
            radius_overlay: 10.0,
            border_control_w: 1.0,
            topbar_divider: false,
            switch_pill: true,
            wireframe_controls: false,
            mono_headers: false,

            bg_shell: rgb(0x12, 0x13, 0x16),
            bg_sidebar: rgb(0x18, 0x19, 0x1d),
            bg_elevated: rgb(0x22, 0x24, 0x2a),
            bg_input: rgb(0x10, 0x11, 0x14),
            bg_hover: rgba(0xff, 0xff, 0xff, 0x10),
            bg_active: rgba(0xff, 0xff, 0xff, 0x1c),
            bg_popover: rgba(0x1c, 0x1d, 0x22, 0xdc),
            bg_toast: rgba(0x1c, 0x1d, 0x22, 0xf4),
            bg_toggle_off: rgb(0x2d, 0x30, 0x38),
            bg_ruler: rgb(0x15, 0x16, 0x1a),
            bg_guide: rgb(0x24, 0x26, 0x2d),

            divider: rgba(0xff, 0xff, 0xff, 0x13),
            border_control: rgba(0xff, 0xff, 0xff, 0x18),
            border_specular: rgba(0xff, 0xff, 0xff, 0x24),

            accent: rgb(0x0a, 0x84, 0xff),
            accent_hover: rgb(0x28, 0x94, 0xff),
            accent_press: rgb(0x00, 0x6b, 0xe0),
            accent_soft: rgba(0x0a, 0x84, 0xff, 0x26),
            accent_text: rgb(0x93, 0xc5, 0xfd),

            text_primary: rgb(0xf2, 0xf3, 0xf7),
            text_secondary: rgb(0x9f, 0xa4, 0xaf),
            text_dim: rgb(0x69, 0x6e, 0x79),

            axis_x: rgb(0xff, 0x45, 0x3a),
            axis_y: rgb(0x30, 0xd1, 0x58),
            axis_z: rgb(0x0a, 0x84, 0xff),
            gold: rgb(0xff, 0xd6, 0x0a),
            green: rgb(0x30, 0xd1, 0x58),
        },
    }
}

impl DesignSpec {
    pub fn apply_to_slint(&self, th: &Theme) {
        th.set_theme_idx(self.idx);
        th.set_radius_control(self.radius_control);
        th.set_radius_overlay(self.radius_overlay);
        th.set_border_control_w(self.border_control_w);
        th.set_topbar_divider(self.topbar_divider);
        th.set_switch_pill(self.switch_pill);
        th.set_wireframe_controls(self.wireframe_controls);
        th.set_mono_headers(self.mono_headers);

        th.set_bg_shell(self.bg_shell);
        th.set_bg_sidebar(self.bg_sidebar);
        th.set_bg_elevated(self.bg_elevated);
        th.set_bg_input(self.bg_input);
        th.set_bg_hover(self.bg_hover);
        th.set_bg_active(self.bg_active);
        th.set_bg_popover(self.bg_popover);
        th.set_bg_toast(self.bg_toast);
        th.set_bg_toggle_off(self.bg_toggle_off);
        th.set_bg_ruler(self.bg_ruler);
        th.set_bg_guide(self.bg_guide);

        th.set_divider(self.divider);
        th.set_border_control(self.border_control);
        th.set_border_specular(self.border_specular);

        th.set_accent(self.accent);
        th.set_accent_hover(self.accent_hover);
        th.set_accent_press(self.accent_press);
        th.set_accent_soft(self.accent_soft);
        th.set_accent_text(self.accent_text);

        th.set_text_primary(self.text_primary);
        th.set_text_secondary(self.text_secondary);
        th.set_text_dim(self.text_dim);

        th.set_axis_x(self.axis_x);
        th.set_axis_y(self.axis_y);
        th.set_axis_z(self.axis_z);
        th.set_gold(self.gold);
        th.set_green(self.green);
    }
}
