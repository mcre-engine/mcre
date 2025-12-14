use bevy::prelude::*;

use std::fmt::Write;

use crate::chunk::loader::ChunkLoader;

#[derive(Component)]
pub struct ChunkText;

impl ChunkText {
    pub fn into_bundle(self, loader: &ChunkLoader) -> impl Bundle {
        (
            self,
            Text::new(Self::format_text(loader)),
            TextFont {
                font_size: 20.0,
                ..Default::default()
            },
            TextColor(Color::WHITE),
        )
    }

    fn format_text(loader: &ChunkLoader) -> String {
        let mut output = "Chunks:\n".to_owned();

        let _ = writeln!(&mut output, "\tUnloaded: {}", loader.unloaded_chunks());
        let _ = writeln!(&mut output, "\tGenerating: {}", loader.generating_chunks());
        let _ = writeln!(&mut output, "\tRendering: {}", loader.rendering_chunks());
        let _ = writeln!(&mut output, "\tLoaded: {}", loader.loaded_chunks());
        let _ = writeln!(&mut output, "\tSaving: {}", loader.saving_chunks());

        output
    }

    pub fn update_text_system(mut ui: Query<&mut Text, With<ChunkText>>, loader: Res<ChunkLoader>) {
        let mut ui = ui.single_mut().unwrap();
        ui.0 = Self::format_text(&loader);
    }
}
