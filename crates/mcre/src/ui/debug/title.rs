use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use std::fmt::Write;

use crate::chunk::generate::rng::ChunkRng;

const TITLE: &str = concat!("MCRE ", env!("CARGO_PKG_VERSION"));

#[derive(Component)]
pub struct TitleText;

impl TitleText {
    pub fn into_bundle(self) -> impl Bundle {
        (
            self,
            Text::new(TITLE),
            TextFont {
                font_size: 20.0,
                ..Default::default()
            },
            TextColor(Color::WHITE),
        )
    }

    fn format_text(diagnostics: &DiagnosticsStore, rng: &ChunkRng) -> String {
        let mut output = TITLE.to_owned();
        output.push('\n');

        if let Some(fps) = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|fps| fps.smoothed())
        {
            let _ = writeln!(&mut output, "FPS: {fps:.2}",);
        }
        let _ = writeln!(&mut output, "Seed: {}", rng.seed());
        output
    }

    pub fn update_text_system(
        diagnostics: Res<DiagnosticsStore>,
        rng: Res<ChunkRng>,
        mut text: Query<&mut Text, With<TitleText>>,
    ) {
        let mut text = text.single_mut().unwrap();
        **text = Self::format_text(&diagnostics, &rng);
    }
}
