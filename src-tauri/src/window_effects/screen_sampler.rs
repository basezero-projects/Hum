#[cfg(windows)]
use anyhow::Context;
use anyhow::Result;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SampleRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Serialize, Debug, PartialEq)]
pub struct BgLuminance {
    pub luminance: f32,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub trait ScreenSampler {
    fn sample(&self, region: SampleRegion) -> Result<BgLuminance>;
}

const SAMPLE_HEIGHT: u32 = 30;
const SAMPLE_WIDTH_CAP: u32 = 240;
const SAMPLE_GAP_PX: i32 = 20;

pub fn sample_regions(bounds: OverlayBounds) -> [SampleRegion; 2] {
    let width = bounds.width.min(SAMPLE_WIDTH_CAP);
    let x = bounds.x + (bounds.width as i32 - width as i32) / 2;
    [
        SampleRegion {
            x,
            y: bounds.y + bounds.height as i32 + SAMPLE_GAP_PX,
            width,
            height: SAMPLE_HEIGHT,
        },
        SampleRegion {
            x,
            y: bounds.y - SAMPLE_GAP_PX - SAMPLE_HEIGHT as i32,
            width,
            height: SAMPLE_HEIGHT,
        },
    ]
}

pub fn sample_overlay_background(
    sampler: &dyn ScreenSampler,
    bounds: OverlayBounds,
) -> Result<BgLuminance> {
    let [below, above] = sample_regions(bounds);
    sampler.sample(below).or_else(|_| sampler.sample(above))
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemScreenSampler;

#[cfg(windows)]
impl ScreenSampler for SystemScreenSampler {
    fn sample(&self, region: SampleRegion) -> Result<BgLuminance> {
        sample_native(region)
    }
}

#[cfg(windows)]
fn sample_native(region: SampleRegion) -> Result<BgLuminance> {
    use xcap::Monitor;

    let monitors = Monitor::all().context("Monitor::all")?;
    let center_x = region.x + region.width as i32 / 2;
    let center_y = region.y + region.height as i32 / 2;
    let monitor = monitors
        .iter()
        .find(|monitor| {
            let x = monitor.x().unwrap_or(0);
            let y = monitor.y().unwrap_or(0);
            let width = monitor.width().unwrap_or(0) as i32;
            let height = monitor.height().unwrap_or(0) as i32;
            center_x >= x && center_x < x + width && center_y >= y && center_y < y + height
        })
        .context("no monitor contains sample center")?;

    let image = monitor.capture_image().context("capture_image")?;
    let monitor_x = monitor.x().unwrap_or(0);
    let monitor_y = monitor.y().unwrap_or(0);
    let local_x = (region.x - monitor_x).max(0) as u32;
    let local_y = (region.y - monitor_y).max(0) as u32;
    if local_x >= image.width() || local_y >= image.height() {
        anyhow::bail!("sample origin outside image");
    }
    let crop_width = region.width.min(image.width() - local_x);
    let crop_height = region.height.min(image.height() - local_y);
    if crop_width == 0 || crop_height == 0 {
        anyhow::bail!("zero-sized crop");
    }

    let mut red = 0_u64;
    let mut green = 0_u64;
    let mut blue = 0_u64;
    let mut count = 0_u64;
    for y in local_y..local_y + crop_height {
        for x in local_x..local_x + crop_width {
            let pixel = image.get_pixel(x, y);
            red += u64::from(pixel[0]);
            green += u64::from(pixel[1]);
            blue += u64::from(pixel[2]);
            count += 1;
        }
    }
    if count == 0 {
        anyhow::bail!("no pixels sampled");
    }
    let r = (red / count) as u8;
    let g = (green / count) as u8;
    let b = (blue / count) as u8;
    let luminance = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0;
    Ok(BgLuminance { luminance, r, g, b })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn sample_placement_preserves_exact_dimensions_gap_and_centering() {
        let cases = [
            (
                OverlayBounds {
                    x: 100,
                    y: 200,
                    width: 120,
                    height: 60,
                },
                [
                    SampleRegion {
                        x: 100,
                        y: 280,
                        width: 120,
                        height: 30,
                    },
                    SampleRegion {
                        x: 100,
                        y: 150,
                        width: 120,
                        height: 30,
                    },
                ],
            ),
            (
                OverlayBounds {
                    x: 100,
                    y: 200,
                    width: 500,
                    height: 60,
                },
                [
                    SampleRegion {
                        x: 230,
                        y: 280,
                        width: 240,
                        height: 30,
                    },
                    SampleRegion {
                        x: 230,
                        y: 150,
                        width: 240,
                        height: 30,
                    },
                ],
            ),
            (
                OverlayBounds {
                    x: -300,
                    y: -80,
                    width: 400,
                    height: 50,
                },
                [
                    SampleRegion {
                        x: -220,
                        y: -10,
                        width: 240,
                        height: 30,
                    },
                    SampleRegion {
                        x: -220,
                        y: -130,
                        width: 240,
                        height: 30,
                    },
                ],
            ),
        ];
        for (bounds, expected) in cases {
            assert_eq!(sample_regions(bounds), expected);
        }
    }

    struct RecordingSampler {
        calls: RefCell<Vec<SampleRegion>>,
        outcomes: RefCell<Vec<Result<BgLuminance>>>,
    }

    impl ScreenSampler for RecordingSampler {
        fn sample(&self, region: SampleRegion) -> Result<BgLuminance> {
            self.calls.borrow_mut().push(region);
            self.outcomes.borrow_mut().remove(0)
        }
    }

    fn payload() -> BgLuminance {
        BgLuminance {
            luminance: 0.25,
            r: 10,
            g: 20,
            b: 30,
        }
    }

    fn bounds() -> OverlayBounds {
        OverlayBounds {
            x: 10,
            y: 20,
            width: 100,
            height: 50,
        }
    }

    #[test]
    fn successful_below_sample_skips_above() {
        let sampler = RecordingSampler {
            calls: RefCell::new(Vec::new()),
            outcomes: RefCell::new(vec![Ok(payload())]),
        };
        assert_eq!(
            sample_overlay_background(&sampler, bounds()).unwrap(),
            payload()
        );
        assert_eq!(
            sampler.calls.borrow().as_slice(),
            &sample_regions(bounds())[..1]
        );
    }

    #[test]
    fn failed_below_sample_tries_above_in_order() {
        let sampler = RecordingSampler {
            calls: RefCell::new(Vec::new()),
            outcomes: RefCell::new(vec![Err(anyhow::anyhow!("below")), Ok(payload())]),
        };
        assert_eq!(
            sample_overlay_background(&sampler, bounds()).unwrap(),
            payload()
        );
        assert_eq!(sampler.calls.borrow().as_slice(), &sample_regions(bounds()));
    }

    #[test]
    fn two_failed_samples_return_error() {
        let sampler = RecordingSampler {
            calls: RefCell::new(Vec::new()),
            outcomes: RefCell::new(vec![
                Err(anyhow::anyhow!("below")),
                Err(anyhow::anyhow!("above")),
            ]),
        };
        assert!(sample_overlay_background(&sampler, bounds()).is_err());
        assert_eq!(sampler.calls.borrow().as_slice(), &sample_regions(bounds()));
    }
}
