const VID_3DS: u16 = 0x16D0;
const PID_3DS: u16 = 0x06A3;
use std::time::Duration;
use std::time::SystemTime;

use rusb::{Device, DeviceHandle, GlobalContext};
// Might be a better way to specify this
// const BULK_ENDPOINT_ADDRESS: u8 = 130;
const DEFAULT_TIMEOUT: Duration = Duration::from_millis(500);
const VEND_OUT_REQ: u8 = 0x40;
const VEND_OUT_VALUE: u16 = 0;
const VEND_OUT_IDX: u16 = 0;

// Could give more specific explanation here.
// const FULL_BUFF_SIZE: usize = 522500;
const FULL_BUFF_SIZE: usize = 523500;
const VIDEO_BUFFER_SIZE: usize = 518400;
// const EXTENDED_VID_BUFFER_SIZE: usize = 550000;

#[derive(Debug, Clone)]
struct Endpoint {
    config: u8,
    iface: u8,
    setting: u8,
    address: u8,
}

impl Endpoint {
    pub fn new(config: u8, iface: u8, setting: u8, address: u8) -> Self {
        Self {
            config,
            iface,
            setting,
            address,
        }
    }
}

struct DS {
    device: Device<GlobalContext>,
    handle: DeviceHandle<GlobalContext>,
}

impl DS {
    pub fn new(device: Device<GlobalContext>, handle: DeviceHandle<GlobalContext>) -> Self {
        Self { device, handle }
    }
}

fn get_3ds_device_and_handle() -> Result<DS, anyhow::Error> {
    // Not sure why I still need unwraps here.
    let device = rusb::devices()
        .unwrap()
        .iter()
        .find(|dvc| {
            // Remove unwrpaps
            let desc = dvc.device_descriptor().unwrap();
            desc.vendor_id() == VID_3DS && desc.product_id() == PID_3DS
        })
        .ok_or(anyhow::Error::msg("unable to find 3ds device"))
        .unwrap();

    let handle = rusb::open_device_with_vid_pid(VID_3DS, PID_3DS)
        .ok_or(anyhow::Error::msg("unable to retrieve device handle"))
        .unwrap();

    Ok(DS::new(device, handle))
}

fn get_endpoint(device: rusb::Device<rusb::GlobalContext>) -> Result<Endpoint, anyhow::Error> {
    let device_desc = device.device_descriptor().unwrap();
    println!("Max supported USB Version: {}", device_desc.usb_version());

    // AFAIK there is only one bulk endpoint, but could be improved to check.
    let config_desc = match device.config_descriptor(0) {
        Ok(cd) => cd,
        Err(e) => {
            return Err(anyhow::Error::msg(format!(
                "unable to get config descriptor: {}",
                e
            )))
        }
    };

    let interface = match config_desc.interfaces().last() {
        Some(iface) => iface,
        None => return Err(anyhow::Error::msg("unable to retrieve interface")),
    };

    let interface_desc = match interface.descriptors().last() {
        Some(id) => id,
        None => {
            return Err(anyhow::Error::msg(
                "unable to retrieve inferface description",
            ))
        }
    };

    let endpoint_desc = match interface_desc.endpoint_descriptors().last() {
        Some(ed) => ed,
        None => {
            return Err(anyhow::Error::msg(
                "unable to retrieve endpoint description",
            ))
        }
    };

    Ok(Endpoint::new(
        config_desc.number(),
        interface_desc.interface_number(),
        interface_desc.setting_number(),
        endpoint_desc.address(),
    ))
}

// Might just be my screen but could be worth brightening this too.
// this wont really be capture screen anymore
fn capture_screen(
    handle: &DeviceHandle<GlobalContext>,
    endpoint: &Endpoint,
    window: &mut minifb::Window,
    // sink: &mut rodio::Sink,
) {
    // Should be able to clean this up so we don't need to duplicate the buffer.
    // let max_buffer_size = 525000;

    let vend_out_buff = [0u8; 512];
    let vend_out_req_type = rusb::request_type(
        rusb::Direction::Out,
        rusb::RequestType::Vendor,
        rusb::Recipient::Device,
    );

    // Probably need to handle this because it could fail on any vend
    handle
        .write_control(
            vend_out_req_type,
            VEND_OUT_REQ,
            VEND_OUT_VALUE,
            VEND_OUT_IDX,
            &vend_out_buff,
            DEFAULT_TIMEOUT,
        )
        .expect("unable to vend out to device");

    let mut combined_buff = vec![0u8; FULL_BUFF_SIZE];

    loop {
        match handle.read_bulk(endpoint.address, &mut combined_buff, DEFAULT_TIMEOUT) {
            Ok(bytes_rec) => {
                if bytes_rec == 0 {
                    break;
                }
            }
            Err(err) => {
                eprintln!("unable to read from bulk endpoint: {}", err);
                break;
            }
        }
    }

    // 50% framerate improvement!
    let (vid_buff, _audio_buff) = combined_buff
        .split_first_chunk::<VIDEO_BUFFER_SIZE>()
        .expect("couldnt break buffers");

    let width = 240;
    let height = 720;

    // Wonder if there's a way we can avoid the rotation. Or continue building the buffer without blocking the draw.
    let vid_buf_32 = u8_to_u32(vid_buff);
    let rotated_vid_buf = rotate_270(&vid_buf_32, width, height);
    window
        .update_with_buffer(&rotated_vid_buf, height, width)
        .unwrap();
}

// ROTATING CODE
fn rotate_270(buffer: &[u32], width: usize, height: usize) -> Vec<u32> {
    let mut rotated_buffer = vec![0; width * height];

    for y in 0..height {
        for x in 0..width {
            // Rotate 270 degrees (counterclockwise)
            let rotated_x = y;
            let rotated_y = width - 1 - x;

            // Map (x, y) from the original to the rotated position
            rotated_buffer[rotated_x + rotated_y * height] = buffer[x + y * width];
        }
    }

    rotated_buffer
}

// CHUNKING CODE
fn u8_to_u32(u8_buffer: &[u8]) -> Vec<u32> {
    let mut u32_buffer = Vec::with_capacity(u8_buffer.len() / 3);
    for chunk in u8_buffer.chunks(3) {
        if chunk.len() == 3 {
            let r = chunk[0] as u32;
            let g = chunk[1] as u32;
            let b = chunk[2] as u32;
            let alpha = 255; // Code max opacity

            let px = (alpha << 24) | (r << 16) | (g << 8) | b;

            u32_buffer.push(px);
        } else {
            println!("chunk not complete");
            println!("{:?}", chunk);
        }
    }

    u32_buffer
}

fn main() {
    // Need to actually unwrap errors below this.
    let ds = get_3ds_device_and_handle().expect("unable to locate 3ds device");
    let endpoint = get_endpoint(ds.device).unwrap();

    let using_kernel_driver = match ds.handle.kernel_driver_active(endpoint.iface) {
        Ok(true) => {
            ds.handle.detach_kernel_driver(endpoint.iface).unwrap();
            true
        }
        _ => false,
    };

    ds.handle.set_active_configuration(endpoint.config).unwrap();
    ds.handle.claim_interface(endpoint.iface).unwrap();
    ds.handle
        .set_alternate_setting(endpoint.iface, endpoint.setting)
        .unwrap();

    // This can be made arbitrarily large as long as it's a multiple of 720x240.
    let mut window =
        minifb::Window::new("Test", 1440, 480, minifb::WindowOptions::default()).unwrap();

    let start_time = SystemTime::now();
    let mut fps: u64 = 0;
    let mut last_reported_sec: f32 = 0.0;

    // See https://docs.rs/minifb/latest/i686-pc-windows-msvc/src/minifb/lib.rs.html#533-535
    window.set_target_fps(60);

    // This improved framerates up to about 30fps.
    while window.is_open() && !window.is_key_down(minifb::Key::Escape) {
        capture_screen(&ds.handle, &endpoint, &mut window);

        // Framerate reporting
        fps += 1;
        let current_time = SystemTime::now();
        let elapsed_secs = current_time
            .duration_since(start_time)
            .unwrap()
            .as_secs_f32();
        if elapsed_secs - last_reported_sec >= 1.0 {
            println!("Frames/second: {:?}", fps);
            last_reported_sec = elapsed_secs;
            fps = 0;
        }
    }

    ds.handle.release_interface(endpoint.iface).unwrap();
    if using_kernel_driver {
        ds.handle.attach_kernel_driver(endpoint.iface).unwrap();
    };
}
