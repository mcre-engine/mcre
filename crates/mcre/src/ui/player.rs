use bevy::{color::palettes::css::GRAY, prelude::*};

#[derive(Component)]
pub struct PlayerUi;

const LINE_HEIGHT: f32 = 24.;
const LINE_WIDTH: f32 = 4.;

impl PlayerUi {
    pub fn spawn(commands: &mut Commands) {
        commands
            .spawn(PlayerUi.into_bundle())
            .with_children(|parent| {
                parent.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(LINE_HEIGHT),
                        height: Val::Px(LINE_WIDTH),
                        ..Default::default()
                    },
                    BackgroundColor(GRAY.into()),
                ));
                parent.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(LINE_WIDTH),
                        height: Val::Px(LINE_HEIGHT),
                        ..Default::default()
                    },
                    BackgroundColor(GRAY.into()),
                ));
            });
    }

    fn into_bundle(self) -> impl Bundle {
        (
            self,
            Node {
                height: Val::Percent(100.),
                width: Val::Percent(100.),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..Default::default()
            },
        )
    }
}
