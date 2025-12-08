use bevy::{
    color::palettes::{css::GRAY, tailwind::GREEN_500},
    prelude::*,
};

use crate::{LoadingState, chunk::loader::ChunkLoader, textures::BlockTextures};

const MARGIN: Val = Val::Px(12.);

#[derive(Component)]
pub struct LoadingUi;

impl LoadingUi {
    pub fn into_bundle(self) -> impl Bundle {
        (
            self,
            Node {
                width: Val::Percent(100.),
                height: Val::Percent(100.),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                padding: UiRect::all(MARGIN),
                row_gap: MARGIN,
                ..Default::default()
            },
        )
    }

    pub fn add_ui_system(mut commands: Commands) {
        commands.spawn(Self.into_bundle()).with_children(|parent| {
            parent.spawn((
                Text::new("Loading..."),
                TextFont {
                    font_size: 20.0,
                    ..Default::default()
                },
                TextColor(Color::WHITE),
                LoadText,
            ));
            parent
                .spawn((
                    Node {
                        width: Val::Vw(LOAD_BAR_MAX),
                        height: Val::Vw(2.),
                        justify_content: JustifyContent::FlexStart,
                        align_items: AlignItems::FlexStart,
                        ..Default::default()
                    },
                    BackgroundColor(GRAY.into()),
                ))
                .with_child(LoadBar.into_bundle());
        });
    }

    pub fn update_ui_system(
        mut text: Query<&mut Text, With<LoadText>>,
        mut load: Query<&mut Node, With<LoadBar>>,
        state: Res<State<LoadingState>>,
        textures: Res<BlockTextures>,
        loader: Res<ChunkLoader>,
    ) {
        match state.get() {
            LoadingState::Camera => {}
            LoadingState::Textures => {
                let mut load = load.single_mut().unwrap();
                let mut text = text.single_mut().unwrap();
                text.0 = "Loading Textures...".to_owned();
                load.width = Val::Vw(LOAD_BAR_MAX * textures.loading_percent());
            }
            LoadingState::Chunks => {
                let mut text = text.single_mut().unwrap();
                text.0 = format!(
                    "Loading Chunks: Unloaded - {}, Rendering - {}",
                    loader.unloaded_chunks(),
                    loader.rendering_chunks()
                );
            }
        }
    }

    pub fn remove_ui_system(mut commands: Commands, ui: Query<Entity, With<Self>>) {
        if let Ok(entity) = ui.single() {
            commands.entity(entity).despawn();
        }
    }
}

const LOAD_BAR_MAX: f32 = 60.;

#[derive(Component)]
pub struct LoadBar;

#[derive(Component)]
pub struct LoadText;

impl LoadBar {
    fn into_bundle(self) -> impl Bundle {
        (
            self,
            Node {
                width: Val::Vw(0.),
                height: Val::Vw(2.),
                ..Default::default()
            },
            BackgroundColor(GREEN_500.into()),
        )
    }
}
