use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, sleep},
    time::{Duration, Instant},
};

use generic_camera::{AnyGenCam, GenCamDriver};

use chrono::Utc;
use generic_camera_asi::{
    controls::{AnalogCtrl, DeviceCtrl, ExposureCtrl, SensorCtrl},
    GenCamCtrl, GenCamDriverAsi, GenCamError, GenCamPixelBpp, GenCamRoi, PropertyValue,
};
use log::warn;
use refimage::GenericImageOwned;
#[allow(unused_imports)]
use refimage::{
    CalcOptExp, DemosaicMethod, DynamicImage, FitsCompression, FitsWrite, GenericImage, ImageProps,
    OptimumExposureBuilder, ToLuma,
};

use packet::{CameraCommand, CameraConfig, Outgoing, TempReadout};
#[cfg(any(feature = "uhubctl_pi", feature = "uhubctl_toradex"))]
use std::process::Command;
use tokio::sync::broadcast;

use crate::REFCLK;

pub async fn camera_thread(
    main_run: Arc<AtomicBool>,
    cfg: &mut CameraConfig,
    data_sender: broadcast::Sender<Outgoing>,
    config_source: broadcast::Receiver<CameraCommand>,
) {
    let mut data_sender = data_sender;
    #[cfg(any(feature = "uhubctl_pi", feature = "uhubctl_toradex"))]
    let uhubctl = {
        let cmd = if cfg!(feature = "uhubctl_toradex") {
            Command::new("uhubctl").arg("-l1").output()
        } else {
            Command::new("uhubctl").arg("-l2").output()
        };
        match cmd {
            Ok(_) => {
                log::info!("uhubctl exists");
                false
            }
            Err(e) => {
                log::info!("Error starting uhubctl: {:#?}", e);
                true
            }
        }
    };

    // main loop
    let tsink_main = data_sender.clone();
    while main_run.load(Ordering::SeqCst) {
        let mut cfs = config_source.resubscribe();
        let mut drv = GenCamDriverAsi;
        let num_cameras = drv.available_devices();
        log::info!("Found {} cameras", num_cameras);
        if num_cameras == 0 {
            return;
        }

        let sub_run = Arc::new(AtomicBool::new(true));

        let mut cam = {
            if let Some(cam_name) = &cfg.name {
                log::info!("Connecting to camera: {}", cam_name);
                let devlist = drv.list_devices().expect("Could not list devices");
                let dev = devlist
                    .iter()
                    .find(|d| d.name.contains(cam_name))
                    .expect("Could not find camera");
                drv.connect_device(dev).expect("Error connecting to camera")
            } else {
                drv.connect_first_device()
                    .expect("Error connecting to camera")
            }
        };

        let caminfo = cam.info_handle().expect("Error getting camera handle");
        let (sender, receiver) = std::sync::mpsc::channel();
        let camthread = {
            let main_run = main_run.clone();
            let sub_run = sub_run.clone();
            thread::spawn({
                let sink = tsink_main.clone();
                move || {
                    while sub_run.load(Ordering::SeqCst) && main_run.load(Ordering::SeqCst) {
                        // let caminfo = cam;
                        sleep(Duration::from_secs(1));
                        if let Ok(cmd) = cfs.try_recv() {
                            sender.send(cmd).unwrap();
                        }
                        let (temp, _) = caminfo
                            .get_property(GenCamCtrl::Device(DeviceCtrl::Temperature))
                            .unwrap_or((PropertyValue::from(-273.15), false));
                        let dtime = Instant::now();
                        let meas = TempReadout {
                            now: dtime - *REFCLK,
                            readings: (
                                "CCD".to_string(),
                                ((temp.clone().try_into().unwrap_or(-273.15) as f32) * 1000.0)
                                    .round() as i32,
                            ),
                        };
                        if let Err(e) = sink.send(meas.into()) {
                            log::warn!("Error sending temp readout: {:#?}", e);
                        }
                        // let stdout = io::stdout();
                        // let _ = write!(&mut stdout.lock(),
                        log::info!(
                            "Camera temperature: {:>+05.1} C, Cooler Power: {:>3}%\t",
                            temp.try_into().unwrap_or(-273.15),
                            caminfo
                                .get_property(GenCamCtrl::Device(DeviceCtrl::CoolerPower))
                                .unwrap_or((PropertyValue::from(-1i64), false))
                                .0
                                .try_into()
                                .unwrap_or(-1i64)
                        );
                    }
                    if let Err(e) = caminfo.cancel_capture() {
                        log::warn!("Error cancelling capture: {:#?}", e);
                    }
                    log::info!("Exiting housekeeping thread");
                }
            })
        };
        image_capture(
            main_run.clone(),
            sub_run,
            &mut cam,
            cfg,
            &mut data_sender,
            receiver,
            uhubctl,
        )
        .await;
        camthread.join().unwrap();
    }
    log::info!("Exiting");
}

async fn image_capture(
    main_run: Arc<AtomicBool>,
    sub_run: Arc<AtomicBool>,
    cam: &mut AnyGenCam,
    cfg: &mut CameraConfig,
    data_sender: &mut broadcast::Sender<Outgoing>,
    rcv: std::sync::mpsc::Receiver<CameraCommand>,
    uhubctl: bool,
) {
    let info = cam.info().expect("Error getting camera info").clone();

    if let Some(color) = info.info.get("Color Sensor") {
        if let Some(color) = color.as_bool() {
            if !color {
                log::info!("Setting pixel format to 16-bit");
                cam.set_property(
                    SensorCtrl::PixelFormat.into(),
                    &GenCamPixelBpp::Bpp16.into(),
                    false,
                )
                .expect("Error setting pixel format");
            }
        }
    }

    let mut rcmd = None;
    let mut magic = 0;
    let mut ncfg = cfg.clone();
    'cfg_loop: while main_run.load(Ordering::Relaxed) {
        let mut cmd_rcv = false;
        let mut cmd_success = true;
        if let Some(cmd) = rcmd.take() {
            cmd_rcv = true;
            match cmd {
                CameraCommand::Roi(m, roi) => {
                    ncfg.roi = roi;
                    magic = m;
                    log::info!("Received ROI command: {:#?}", ncfg.roi);
                }
                CameraCommand::ExposureConf(m, expconf) => {
                    magic = m;
                    ncfg.autoexp = expconf;
                    log::info!("Received exposure command: {:#?}", ncfg.autoexp);
                }
                CameraCommand::Settings(m, settings) => {
                    magic = m;
                    ncfg.settings = settings;
                    log::info!("Received settings command: {:#?}", ncfg.settings);
                }
                CameraCommand::FullConf(m, fcfg) => {
                    magic = m;
                    ncfg = fcfg;
                    log::info!("Received full config command: {:#?}", ncfg);
                }
            }
        }
        log::info!(
            "Setting target temperature: {} C",
            ncfg.settings.target_temp
        );
        if cam
            .set_property(
                GenCamCtrl::Device(DeviceCtrl::CoolerTemp),
                &PropertyValue::Int(ncfg.settings.target_temp as i64),
                false,
            )
            .is_err()
        {
            cmd_success = false;
            log::info!("Error setting target temperature");
        }

        if ncfg.roi.change_roi() {
            let roi = cam.get_roi();
            log::info!(
                "Current ROI: {}x{} @ {}x{}",
                roi.width,
                roi.height,
                roi.x_min,
                roi.y_min
            );
            if let Err(e) = cam.set_roi(&GenCamRoi {
                width: (ncfg.roi.x_max - ncfg.roi.x_min) as _,
                height: (ncfg.roi.y_max - ncfg.roi.y_min) as _,
                x_min: ncfg.roi.x_min as _,
                y_min: ncfg.roi.y_min as _,
            }) {
                cmd_success = false;
                ncfg.roi = cfg.roi.clone(); // reset to default
                log::error!("Error setting ROI: {:#?}", e);
            }
            let roi = cam.get_roi();
            log::info!(
                "New ROI: {}x{} @ {}x{}",
                roi.width,
                roi.height,
                roi.x_min,
                roi.y_min
            );
        }
        let mut current_exp = Duration::from_millis(100);
        cam.set_property(
            GenCamCtrl::Exposure(ExposureCtrl::ExposureTime),
            &(Duration::from_millis(100).into()),
            false,
        )
        .expect("Error setting exposure time");
        // gain settings
        if let Some(prop) = cam.list_properties().get(&AnalogCtrl::Gain.into()) {
            log::info!("Gain Settings: {:#?}", prop);
        }
        if let Ok((gain, auto)) = cam.get_property(AnalogCtrl::Gain.into()) {
            log::info!(
                "Current gain: {:.1} dB, Auto mode: {}",
                gain.as_f64().unwrap_or(-1.0),
                auto
            );
        }
        if let Some(gain) = ncfg.settings.gain {
            if let Err(e) = cam.set_property(AnalogCtrl::Gain.into(), &gain.into(), false) {
                cmd_success = false;
                log::error!("Error setting gain: {:#?}", e);
            } else {
                log::info!("Setting gain to {:.1} dB", gain);
            }
        } else {
            // set optimal gain for the cameras we use
            if info.name.contains("533") {
                if let Err(e) = cam.set_property(AnalogCtrl::Gain.into(), &10.0f64.into(), false) {
                    log::warn!("Error setting camera gain: {e:#?}");
                } else {
                    log::info!("Setting {} gain to 10 dB", &info.name);
                }
            } else if info.name.contains("432") {
                if let Err(e) = cam.set_property(AnalogCtrl::Gain.into(), &14.0f64.into(), false) {
                    log::warn!("Error setting camera gain: {e:#?}");
                } else {
                    log::info!("Setting {} gain to 14 dB", &info.name);
                }
            } else if info.name.contains("585") {
                if let Err(e) = cam.set_property(AnalogCtrl::Gain.into(), &25.2f64.into(), false) {
                    log::warn!("Error setting camera gain: {e:#?}");
                } else {
                    log::info!("Setting {} gain to 25.2 dB", &info.name);
                }
            }
        }
        // change to 8 bit?
        if ncfg.settings.pix8b {
            log::info!("Setting pixel format to 8-bit");
            if let Err(e) = cam.set_property(
                SensorCtrl::PixelFormat.into(),
                &GenCamPixelBpp::Bpp8.into(),
                false,
            ) {
                cmd_success = false;
                log::error!("Error setting pixel format: {:#?}", e);
                ncfg.settings.pix8b = false; // reset to default
            }
        }

        let props = cam.list_properties();
        let exp_prop = props
            .get(&GenCamCtrl::Exposure(ExposureCtrl::ExposureTime))
            .expect("Error getting exposure property");
        let mut exp_ctrl = ncfg.autoexp.clone().get_controller();
        if let Ok(min_exp) = exp_prop.get_min() {
            let min_exp = min_exp.as_duration().unwrap();
            let min_exp = min_exp.min(ncfg.autoexp.min_allowed_exp);
            ncfg.autoexp.min_allowed_exp = min_exp;
            exp_ctrl = exp_ctrl.min_allowed_exp(min_exp);
        }
        if let Ok(max_exp) = exp_prop.get_max() {
            let max_exp = max_exp.as_duration().unwrap();
            let max_exp = max_exp.min(ncfg.autoexp.max_allowed_exp);
            ncfg.autoexp.max_allowed_exp = max_exp;
            exp_ctrl = exp_ctrl.max_allowed_exp(max_exp);
        }
        let exp_ctrl = match exp_ctrl.build() {
            Ok(exp_ctrl) => exp_ctrl,
            Err(e) => {
                cmd_success = false;
                log::error!("Error building exposure controller: {:#?}", e);
                let mut exp_ctrl = cfg.autoexp.clone().get_controller();
                if let Ok(min_exp) = exp_prop.get_min() {
                    exp_ctrl = exp_ctrl.min_allowed_exp(min_exp.as_duration().unwrap());
                }
                let exp_ctrl = exp_ctrl
                    .build()
                    .expect("Error building exposure controller"); // fallback
                ncfg.autoexp = cfg.autoexp.clone(); // reset to default
                exp_ctrl
            }
        };
        if cmd_rcv {
            let pack = if cmd_success {
                Outgoing::Ack(magic)
            } else {
                Outgoing::Nack(magic)
            };
            if let Err(e) = data_sender.send(pack) {
                log::error!("Error sending command ack: {:#?}", e);
            }
        }
        let mut last_saved = None;
        'exposure_loop: while main_run.load(Ordering::Relaxed) && sub_run.load(Ordering::Relaxed) {
            let exp_start = Utc::now();
            let exp_inst = Instant::now();
            if cam.start_exposure().is_err() {
                log::error!("Error starting exposure");
                break 'exposure_loop;
            }
            // wait for the exposure to finish
            tokio::time::sleep(current_exp).await;
            // check if the exposure was cancelled
            while main_run.load(Ordering::Relaxed) && sub_run.load(Ordering::Relaxed) {
                match cam.image_ready() {
                    Err(e) => match e {
                        GenCamError::TimedOut => {
                            log::warn!("<{}> AERO: Timeout", exp_start.format("%H:%M:%S"));
                            continue;
                        }
                        GenCamError::ExposureNotStarted => {
                            // probably ctrl + c was pressed
                            continue;
                        }
                        GenCamError::ExposureFailed(reason) => {
                            log::error!("Error capturing image: {}, re-enumerating...", reason);
                            sub_run.store(false, Ordering::Relaxed); // indicate to stop the housekeeping thread
                            #[cfg(feature = "uhubctl_pi")]
                            {
                                if uhubctl {
                                    if let Err(e) =
                                        Command::new("uhubctl").arg("-aoff").arg("-l2").output()
                                    {
                                        log::error!("Error turning off USB hub: {:#?}", e);
                                    }
                                    sleep(Duration::from_secs(10));
                                    if let Err(e) =
                                        Command::new("uhubctl").arg("-aon").arg("-l2").output()
                                    {
                                        log::error!("Error turning on USB hub: {:#?}", e);
                                    }
                                    sleep(Duration::from_secs(10));
                                }
                            }
                            #[cfg(feature = "uhubctl_toradex")]
                            {
                                if uhubctl {
                                    if let Err(e) =
                                        Command::new("uhubctl").arg("-aoff").arg("-l1").output()
                                    {
                                        log::error!("Error turning off USB hub: {:#?}", e);
                                    }
                                    sleep(Duration::from_secs(5));
                                    if let Err(e) =
                                        Command::new("uhubctl").arg("-aon").arg("-l1").output()
                                    {
                                        log::error!("Error turning on USB hub: {:#?}", e);
                                    }
                                    sleep(Duration::from_secs(5));
                                }
                            }
                            break 'exposure_loop; // re-initialize the camera
                        }
                        _ => {
                            panic!("Error capturing image: {:?}", e);
                        }
                    },
                    Ok(true) => {
                        break;
                    }
                    Ok(false) => {
                        // image not ready yet
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                }
            }
            // download the image
            let mut img = match cam.download_image() {
                Ok(img) => img,
                Err(e) => {
                    log::error!("Error downloading image: {:#?}", e);
                    break 'exposure_loop;
                }
            };
            // insert the delta time key
            if let Err(e) = img.insert_key(
                "DTIME",
                (exp_inst - *REFCLK, "Time elapsed since program start"),
            ) {
                warn!("Error inserting DTIME key: {:#?}", e);
            }
            let mut img: GenericImage = img.into();
            // check if it is time to send a new image
            let save = match last_saved {
                None => true,
                Some(last_saved) => {
                    let elapsed = Instant::now().duration_since(last_saved);
                    elapsed > ncfg.settings.cadence
                }
            };
            if save {
                last_saved = Some(Instant::now());
                // send the image to the clients
                if data_sender
                    .send({
                        let img = GenericImageOwned::from(img.clone());
                        Outgoing::ImageData(img)
                    })
                    .is_err()
                {
                    warn!("\nError sending image to clients");
                }
            }
            // check for incoming commands
            if let Ok(cmd) = rcv.try_recv() {
                rcmd = Some(cmd);
                log::info!("Received command: {:#?}", rcmd);
            }
            // if the image has a valid exposure time, calculate the optimal exposure
            if let Some(exp) = img.get_exposure() {
                // convert the image to grayscale
                img.to_luma().expect("Error converting image to grayscale");
                // calculate the optimal exposure
                let (opt_exp, _) = img
                    .calc_opt_exp(&exp_ctrl, exp, 1)
                    .expect("Could not calculate optimal exposure");
                if opt_exp != exp {
                    log::info!(
                        "<{}> AERO: Exposure changed from {:.6} s to {:.6} s",
                        exp_start.format("%H:%M:%S"),
                        exp.as_secs_f32(),
                        opt_exp.as_secs_f32()
                    );
                    if cam
                        .set_property(
                            GenCamCtrl::Exposure(ExposureCtrl::ExposureTime),
                            &opt_exp.into(),
                            false,
                        )
                        .is_ok()
                    {
                        current_exp = opt_exp;
                    } else {
                        log::error!("Error setting exposure time");
                    }
                }
            } else {
                log::error!(
                    "<{}> AERO: No exposure value found",
                    exp_start.format("%H:%M:%S")
                );
            }
            if rcmd.is_some() {
                continue 'cfg_loop;
            }
        }
    }
    *cfg = ncfg; // update the config with the new settings
    log::info!("Exiting image capture function");
}
