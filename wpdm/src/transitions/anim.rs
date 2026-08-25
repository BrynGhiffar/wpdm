use std::ops::Div;

use rand::Rng;
use wpdm_common::TransitionOrigin;

use crate::transitions::{circle::{CircleTransition, CircleTransitionOpt}, initial::NoTransition, wipe::{WipeTransition, WipeTransitionOpt}};

#[derive(Debug, Clone)]
pub enum TransitionAnim {
    Circle(CircleTransition),
    Wipe(WipeTransition),
    None(NoTransition),
}

impl TransitionAnim {
    pub fn circle(width: u32, height: u32, origin: TransitionOrigin) -> Self {
        let width_f32 = width as f32;
        let height_f32 = height as f32;
        let center_x = width_f32.div(2.0);
        let center_y = height_f32.div(2.0);
        let n_frames = 60;
        let opt = match origin {
            TransitionOrigin::Center => CircleTransitionOpt { origin_x: center_x, origin_y: center_y, n_frames },
            TransitionOrigin::Left => CircleTransitionOpt { origin_x: 0.0, origin_y: center_y, n_frames },
            TransitionOrigin::Right => CircleTransitionOpt { origin_x: width_f32, origin_y: center_y, n_frames },
            TransitionOrigin::Coord(cx, cy) => CircleTransitionOpt { origin_x: cx as f32, origin_y: cy as f32, n_frames },
            TransitionOrigin::Random => {
                let mut rng = rand::rng();
                let rand_x: u32 = rng.random_range(0..=width);
                let rand_y: u32 = rng.random_range(0..=height);
                CircleTransitionOpt { origin_x: rand_x as f32, origin_y: rand_y as f32, n_frames }
            }
        };
        Self::Circle(CircleTransition::new_opt(width, height, Some(opt)))
    }

    pub fn wipe(width: u32, height: u32, angle: f32, origin: TransitionOrigin) -> Self {
        let n_frames = 60;
        let angle = {
            if origin == TransitionOrigin::Random {
                let mut rng = rand::rng();
                let angle: u32 = rng.random_range(0..=90);
                angle as f32
            } else {
                angle
            }
        };
        Self::Wipe(WipeTransition::new_opt(width, height, Some(WipeTransitionOpt { n_frames, angle })))
    }

    pub fn none(width: u32, height: u32) -> Self {
        Self::None(NoTransition::new(width, height))
    }

    pub fn render(&self, frame: u32, from: &[u8], to: &[u8], result: &mut [u8]) -> bool {
        match self {
            Self::Wipe(wipe) => wipe.render(frame, from, to, result),
            Self::Circle(circle) => circle.render(frame, from, to, result),
            Self::None(no_tran) => no_tran.render(frame, from, to, result)
        }
    }
}

