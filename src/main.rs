const VID_3DS: u16 = 0x16D0;
const PID_3DS: u16 = 0x06A3;
use std::time::Duration;

use rusb::{DeviceHandle, GlobalContext};
const BULK_ENDPOINT_ADDRESS: u8 = 130;
const DEFAULT_TIMEOUT: Duration = Duration::from_millis(500);

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

fn get_3ds_device() -> Option<rusb::Device<rusb::GlobalContext>> {
    for device in rusb::devices().unwrap().iter() {
        let device_desc = device.device_descriptor().unwrap();

        if device_desc.vendor_id() == VID_3DS && device_desc.product_id() == PID_3DS {
            return Some(device);
        }
    }

    None
}

// You probably need to combine these fns because they're connected.
fn get_3ds_handle() -> Option<DeviceHandle<GlobalContext>> {
    rusb::open_device_with_vid_pid(VID_3DS, PID_3DS)
}

// Need to add some debugging here - we expect only one endpoint but could be more right?
fn get_endpoint(device: rusb::Device<rusb::GlobalContext>) -> Result<Endpoint, anyhow::Error> {
    let device_desc = device.device_descriptor().unwrap();

    // Iterate over device configurations
    for n in 0..device_desc.num_configurations() {
        let config_desc = match device.config_descriptor(n) {
            Ok(c) => c,
            Err(_) => {
                return Err(anyhow::Error::msg(
                    "unable to retrieve device configuration",
                ));
            }
        };

        // Fix this loop never loops error.
        for interface in config_desc.interfaces() {
            for interface_desc in interface.descriptors() {
                for endpoint_desc in interface_desc.endpoint_descriptors() {
                    if endpoint_desc.address() != BULK_ENDPOINT_ADDRESS {
                        // Interesting. Do we have to type return and semi in a for loop?
                        return Err(anyhow::Error::msg("endpoint was not the bulk endpoint"));
                    } else {
                        return Ok(Endpoint::new(
                            config_desc.number(),
                            interface_desc.interface_number(),
                            interface_desc.setting_number(),
                            endpoint_desc.address(),
                        ));
                    }
                }
            }
        }
    }

    Err(anyhow::Error::msg("failed to find bulk endpoint"))
}

// this wont really be capture screen anymore
fn capture_screen(
    handle: &DeviceHandle<GlobalContext>,
    endpoint: &Endpoint,
    window: &mut minifb::Window,
    // sink: &mut rodio::Sink,
) {
    // Should be able to clean this up so we don't need to duplicate the buffer.
    let max_buffer_size = 525000;
    let buf = [0u8; 512];
    let mut buffer = vec![0u8; max_buffer_size];
    let req = rusb::request_type(
        rusb::Direction::Out,
        rusb::RequestType::Vendor,
        rusb::Recipient::Device,
    );

    // Should also check max USB speed on device.
    // 0x40 should be turned into a constant as part of docs.
    match handle.write_control(req, 0x40, 0, 0, &buf, DEFAULT_TIMEOUT) {
        Ok(res) => println!("Success was {:?}", res),
        Err(err) => println!("Error was {:?}", err),
    };

    let mut full_buff_size = 0;
    loop {
        match handle.read_bulk(endpoint.address, &mut buffer, DEFAULT_TIMEOUT) {
            Ok(len) => {
                if len == 0 {
                    break;
                }
                println!("Received {} bytes of data", len);
                full_buff_size = len
            }
            Err(err) => {
                println!("{:?}", err);
                break;
            }
        };
    }

    let mut video_vec_u8: Vec<u8> = Vec::new();
    let mut audio_vec_u8: Vec<u8> = Vec::new();

    let mut i = 0;

    // Everything after 518400 should be audio
    let video_buff_len = 518400;

    // Yeah this is gross.
    while i < full_buff_size {
        if i < video_buff_len {
            video_vec_u8.push(buffer[i]);
        } else {
            audio_vec_u8.push(buffer[i])
        }
        i += 1;
    }
    println!("Video data length is {}", video_vec_u8.len());
    println!("Audio data length is {}", audio_vec_u8.len());

    let width = 240;
    let height = 720;

    let vid_buf_32 = u8_to_u32(&video_vec_u8);
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
    println!("{:?}", u8_buffer.len());
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
    let device = get_3ds_device().unwrap();
    let handle = get_3ds_handle().unwrap();
    let endpoint = get_endpoint(device).unwrap();

    let using_kernel_driver = match handle.kernel_driver_active(endpoint.iface) {
        Ok(true) => {
            handle.detach_kernel_driver(endpoint.iface).unwrap();
            true
        }
        _ => false,
    };

    handle.set_active_configuration(endpoint.config).unwrap();
    handle.claim_interface(endpoint.iface).unwrap();
    handle
        .set_alternate_setting(endpoint.iface, endpoint.setting)
        .unwrap();

    // This can be made arbitrarily large as long as it's a multiple of 720x240.
    // Initialize window
    let mut window =
        minifb::Window::new("Test", 1440, 480, minifb::WindowOptions::default()).unwrap();

    // Initialize sound
    // let host = rodio::cpal::default_host();
    // let devices = host.output_devices().unwrap();

    // let mut chosen_dev: Option<rodio::Device> = None;

    // for device in devices {
    //     let dev: rodio::Device = device;
    //     let dev_name: String = dev.name().unwrap();
    //     println!(" # Device : {}", dev_name);
    //     chosen_dev = Some(dev);
    // }

    // let chosen_dev = match chosen_dev {
    //     None => panic!("no audio device"),
    //     Some(dev) => dev,
    // };

    // let (_, audio_handle) = rodio::OutputStream::try_from_device(&chosen_dev).unwrap();
    // println!("the chosen device is {:?}", chosen_dev.name());
    // let mut sink = rodio::Sink::try_new(&audio_handle).unwrap();

    while window.is_open() && !window.is_key_down(minifb::Key::Escape) {
        capture_screen(&handle, &endpoint, &mut window);
    }

    handle.release_interface(endpoint.iface).unwrap();
    if using_kernel_driver {
        handle.attach_kernel_driver(endpoint.iface).unwrap();
    };
}
