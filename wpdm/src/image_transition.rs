use std::{path::Path};

use rayon::{iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator}, slice::ParallelSliceMut};

pub struct ImageTransition {
    frame_count: u8,
    velocity: Vec<i8>,
    initial_image: Vec<u8>,
    final_image: Vec<u8>,
    is_finished: bool
}

pub fn get_velocity(a: &[u8], b: &[u8]) -> Vec<i8> {
    a.iter().zip(b).map(|(a, b)| {
        let a = *a as i16;
        let b = *b as i16;
        let v = 1;
        ((b - a).signum() as i8) * v
    })
    .collect()
}

pub fn apply_velocity(initial_img: &[u8], final_img: &[u8], velocity: &[i8], frame: u8) -> Vec<u8> {
    initial_img.par_iter()
        .zip(final_img)
        .zip(velocity)
        .map(|((a, b), v)| {
            let a = *a as i16;
            let b = *b as i16;
            let v = *v as i16;
            let frame = frame as i16;
            let c = a + v * frame;

            let lower = std::cmp::min(a, b);
            let higher = std::cmp::max(a, b);

            std::cmp::max(lower, std::cmp::min(c, higher)) as u8
        })
        .collect()
}

impl ImageTransition {

    fn read_image_as_argb<P: AsRef<Path>>(path: P, width: u32, height: u32) -> anyhow::Result<Vec<u8>> {

        let buff = { 
            let mut current = image::open(path)?;
            if current.width() != width || current.height() != height {
                current = current.resize(
                    width, 
                    height, 
                    image::imageops::FilterType::Triangle
                );
            }
            current.to_rgba8()
        };

        let mut buff = buff.to_vec();

        buff.par_chunks_exact_mut(4).for_each(|buff| {
            let r = buff[0];
            let g = buff[1];
            let b = buff[2];
            let a = buff[3];
            buff.copy_from_slice(&[b, g, r, a]);
        });
        Ok(buff)
    }

    pub fn new(images: &[String]) -> Self {
        let mut it = images.iter();
        let first_image_path =  it.next().expect("Expect two images");
        let second_image_path = it.next().expect("Expect two images");

        let width = 1920;
        let height = 1080;

        let initial_image = Self::read_image_as_argb(first_image_path, width, height).unwrap();
        let final_image = Self::read_image_as_argb(second_image_path, width, height).unwrap();

        assert_eq!(initial_image.len(), final_image.len());

        let velocity = get_velocity(&initial_image, &final_image);

        Self {
            initial_image,
            final_image,
            frame_count: 0,
            is_finished: false,
            velocity
        }
    }

    pub fn is_finished(&self) -> bool {
        self.is_finished
    }

    pub fn get_frame(&mut self) -> Vec<u8> {
        let frame = apply_velocity(
            &self.initial_image, 
            &self.final_image, 
            &self.velocity, 
            self.frame_count
        );

        let next_count = self.frame_count.saturating_add(1);

        if self.frame_count.is_multiple_of(10) || self.frame_count == next_count  {
            self.is_finished = (frame == self.final_image)
                || next_count == self.frame_count;
        }
        self.frame_count = next_count;
        frame
    }
}
