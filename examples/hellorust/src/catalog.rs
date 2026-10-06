//! The demo's sample pages: the single source of the root menu cards, as the React demo's
//! `catalog.ts`. Its 20 entries, titles and texts, in its order.

use drawnui::prelude::*;

use crate::{App, pages};

/// One entry per sample page.
pub struct Sample {
    /// The React demo's route name.
    pub route: &'static str,
    pub title: &'static str,
    pub text: &'static str,
    /// Builds the page content that goes under the shell's nav bar.
    pub build: fn(&mut App) -> Build<SkiaLayout>,
}

/// Every sample, in the order the React demo lists them.
pub static SAMPLES: [Sample; 20] = [
    Sample {
        route: "cells",
        title: "Recycled cells",
        text: "100 000 items in a SkiaScroll, RecyclingTemplate + MeasureFirst, UseCache=Image",
        build: pages::cells::build,
    },
    Sample {
        route: "uneven",
        title: "Uneven cells",
        text: "Rows of different heights — MeasureVisible, LoadMore at both ends, ImageDoubleBuffered cells",
        build: pages::uneven::build,
    },
    Sample {
        route: "images",
        title: "Images",
        text: "SkiaImage — every TransformAspect, alignment, clipping",
        build: pages::images::build,
    },
    Sample {
        route: "svg",
        title: "SVG",
        text: "SkiaSvg — file and inline sources, TintColor, LockRatio",
        build: pages::svg::build,
    },
    Sample {
        route: "shapes",
        title: "Shapes",
        text: "SkiaShape — rectangle, circle, ellipse, arc, polygon, line, path; stroke, corner radii, clipping",
        build: pages::shapes::build,
    },
    Sample {
        route: "text",
        title: "Text",
        text: "SkiaLabel — word wrap, MaxLines, alignment, spans, weights, glyph fallback",
        build: pages::text::build,
    },
    Sample {
        route: "layouts",
        title: "Layouts",
        text: "Every SkiaLayout type — Absolute, Column, Row, Wrap, Grid (tracks, spans, spacing)",
        build: pages::layouts::build,
    },
    Sample {
        route: "looks",
        title: "Common Controls",
        text: "SkiaSwitch, SkiaCheckbox, SkiaRadioButton, SkiaProgress, SkiaSlider, SkiaButton — Default, Windows, Cupertino, Material, Material3",
        build: pages::looks::build,
    },
    Sample {
        route: "snapping",
        title: "Carousel & Drawer",
        text: "SkiaCarousel (swipe, SidesOffset peek, SelectedIndex) and SkiaDrawer (drag from an edge, snap by velocity)",
        build: pages::snapping::build,
    },
    Sample {
        route: "animations",
        title: "Lottie & GIF",
        text: "SkiaLottie (Skottie: AutoPlay, Repeat, SpeedRatio, IsOn, ColorTint) and SkiaGif frames on the canvas frame loop",
        build: pages::animations::build,
    },
    Sample {
        route: "shell",
        title: "Shell",
        text: "SkiaShell — page transitions, OpenPopupAsync, PushModalAsync (drawer), ShowToast",
        build: pages::shell::build,
    },
    Sample {
        route: "editor",
        title: "Editor",
        text: "SkiaEditor — drawn text input: caret, selection, placeholder, password, multiline, ControlStyle looks",
        build: pages::editor::build,
    },
    Sample {
        route: "keyboard",
        title: "Keyboard Input",
        text: "KeyboardManager — window-level KeyDown / KeyUp / KeyChar with modifier state, the Blazor sandbox probe",
        build: pages::keyboard::build,
    },
    Sample {
        route: "scroll",
        title: "SkiaScroll",
        text: "Header in flow / sticky / behind with parallax, Footer, scroll bars, pull to refresh, SnapToChildren, TrackIndexPosition",
        build: pages::scroll::build,
    },
    Sample {
        route: "shaders",
        title: "Shaders",
        text: "SkiaShaderEffect — SkSL on any control (iImage1, iTime, iMouse, custom uniforms, touch ripples) and SkiaShaderCarousel gl-transitions",
        build: pages::shaders::build,
    },
    Sample {
        route: "sprites",
        title: "Sprites",
        text: "SkiaSprite spritesheets and a SkiaSpriteSet warrior on a tile board, moved with the keyboard (FastRepro sprites)",
        build: pages::sprites::build,
    },
    Sample {
        route: "transforms",
        title: "Transforms",
        text: "Rotation, Scale, Skew, Translation, Opacity — hit-testing through them, *ToAsync animations",
        build: pages::transforms::build,
    },
    Sample {
        route: "reorder",
        title: "Drag to reorder",
        text: "Language preferences, Android style: drag one by its grip and it lifts and floats over the list, which reorders live under it keeping its measured heights and its scroll offset, then the drop glides into the new slot",
        build: pages::reorder::build,
    },
    Sample {
        route: "pong",
        title: "Pong",
        text: "DrawnGame: game loop, sprites moved by Left / Top, an AI paddle, keyboard and touch — the .NET Pong sample, field fitted to any screen",
        build: pages::pong::build,
    },
    Sample {
        route: "a11y",
        title: "Accessibility",
        text: "ARIA overlay over the canvas — roles, labels, hints, toggles, live regions, keyboard",
        build: pages::a11y::build,
    },
];
