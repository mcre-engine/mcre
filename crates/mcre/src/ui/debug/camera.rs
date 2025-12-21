use bevy::prelude::*;

use std::fmt::Write;

use crate::chunk::{
    loader::ChunkLoaderConfig,
    math::{pos::ChunkPosition, size::ChunkSize},
};

#[derive(Component)]
pub struct PlayerText;

impl PlayerText {
    pub fn into_bundle(self, pos: &Transform, chunk_size: ChunkSize) -> impl Bundle {
        (
            self,
            Text::new(Self::format_text(pos, chunk_size)),
            TextFont {
                font_size: 20.0,
                ..Default::default()
            },
            TextColor(Color::WHITE),
        )
    }

    fn format_text(transform: &Transform, chunk_size: ChunkSize) -> String {
        let pos = transform.translation;

        let mut output = String::new();

        let _ = writeln!(
            &mut output,
            "XYZ: {:.4} / {:.4} / {:.4}",
            pos.x, pos.y, pos.z
        );
        let _ = writeln!(
            &mut output,
            "Block: {:.0} / {:.0} / {:.0}",
            pos.x, pos.y, pos.z
        );
        let pos = ChunkPosition::from_world_coord(pos, chunk_size);
        let _ = writeln!(&mut output, "Chunk: {:.0} / {:.0}", pos.x, pos.y);
        let forward = transform.forward();
        let (facing, towards) = if forward.x.abs() > forward.z.abs() {
            if forward.x.is_sign_negative() {
                ("west", "negative X")
            } else {
                ("east", "positive X")
            }
        } else if forward.z.is_sign_negative() {
            ("north", "negative Z")
        } else {
            ("south", "positive Z")
        };

        let _ = writeln!(&mut output, "Facing: {facing} (Towards {towards})",);

        output
    }

    pub fn update_text_system(
        mut ui: Query<&mut Text, With<PlayerText>>,
        camera: Query<&Transform, With<Camera>>,
        config: Res<ChunkLoaderConfig>,
    ) {
        let mut ui = ui.single_mut().unwrap();
        let camera = camera.single().unwrap();
        ui.0 = PlayerText::format_text(camera, config.chunk_size);
    }
}
