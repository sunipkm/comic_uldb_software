#![deny(missing_docs)]
#![warn(clippy::all)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
//!   
//! # Generic Camera GUI
//! This is the entry point when compiled to WebAssembly.
//!  

use std::borrow::Cow;
use std::io::{Cursor, prelude::*};
use std::net::TcpStream;
use std::sync::atomic::AtomicBool;
use core::str;
use generic_camera::server::GenSrvValue;
use image::DynamicImage;
use refimage::{DynamicImageOwned, GenericImageOwned};
use eframe::egui;
use eframe::egui::{Visuals, load::Bytes};
use egui::{menu, ImageSource, Ui};
use ewebsock::{WsEvent, WsMessage, WsReceiver, WsSender};
use circular_buffer::CircularBuffer;
use packet::Packet;
// use std::future::Future;
// use rfd::AsyncFileDialog;

pub struct WsBackend {
    ws_sender: WsSender,
    ws_receiver: WsReceiver,
    events: Vec<WsEvent>,
    pub image_events: Option<(Vec<u8>, DynamicImage)>,
    pub new_image_event: AtomicBool,
    message: String,
}

impl WsBackend {
    fn connect(uri: &str, ctx: &Option<egui::Context>) -> Option<WsBackend> {
        let res = if let Some(ctx) = ctx {
            let ctx = ctx.clone();
            let wakeup = move || ctx.request_repaint();
            ewebsock::connect_with_wakeup(uri, Default::default(), wakeup)
        } else {
            ewebsock::connect(uri, Default::default())
        };
        match res {
            Ok((ws_sender, ws_receiver)) => {
                let ws = WsBackend {
                    ws_sender,
                    ws_receiver,
                    events: Vec::new(),
                    image_events: None,
                    new_image_event: AtomicBool::new(false),
                    message: String::new(),
                };
                Some(ws)
            }
            Err(e) => {
                eprintln!("Failed to connect to websocket: {}", e);
                None
            }
        }
    }

    fn close(&mut self) {
        self.ws_sender.close();
    }

    fn ui(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.button("Send Ack").clicked() {
                // Send
                self.ws_sender.send(WsMessage::Binary(bincode::serialize(&Packet::Ack).unwrap()));
            }

            if ui.button("Send NAck").clicked() {
                self.ws_sender.send(WsMessage::Binary(bincode::serialize(&Packet::Nack).unwrap()));
            }

            if ui.button("Send ImgReq").clicked() {
                // TODO: Send image request packet.
                // let pkt = GenCamPacket::new(PacketType::ImgReq, 0, 0, 0, None);
                // Set msg to serialized pkt.
                // let msg = serde_json::to_vec(&pkt).unwrap();
                // Send
                self.ws_sender.send(WsMessage::Ping(b"Image Request".to_vec()));
            }
        });
    }
}

#[derive(Debug, Clone)]
enum DialogType {
    Debug,
    Info,
    Warn,
    Error,
}

impl DialogType {
    fn as_str(&self) -> &str {
        match self {
            DialogType::Debug => "DEBUG",
            DialogType::Info => "INFO",
            DialogType::Warn => "WARN",
            DialogType::Error => "ERROR",
        }
    }
}

// TODO: This is an example for the sake of GUI functionality.
#[derive(Debug, Clone)]
struct CamData {
    name: String,
}

// #[derive(Clone)]
/// Structure to hold all of the GUI data.
pub struct GenCamGUI {
    dialog_type: DialogType,
    modal_message: String,
    dark_mode: bool,

    modal_active: bool,

    comms_stream: Option<TcpStream>,
    // comms_buffer: [u8; 4096],
    // server_connection: bool,
    // connected_cameras: HashMap<String, CamData>,

    data: Option<Bytes>,
    image: Option<DynamicImage>,
    img_uri: &'static str,

    frame: egui::Frame,

    msg_list: CircularBuffer<150, String>,

    // Camera Controls Stuff
    exposure_text_edit: String,
    long_exp_checkbox: bool,
    exposure_slider: f32,
    auto_exp_checkbox: bool,
    min_cam_temp: f32,
    max_cam_temp: f32,
    curr_cam_temp: f32,
    cooler_status: CoolerStatus,
    color_space: ColorSpaceOpt,
    roi: [f32; 4],
    roi_type: ROITypes,
    roi_enabled: bool,
    img_width: i32,
    img_height: i32,

    // Websocket
    /// The URI of the websocket server.
    pub uri: String,
    /// The websocket backend.
    pub ws: Option<WsBackend>,
    /// The egui context.
    pub ctx: Option<egui::Context>,
}

#[derive(Debug, PartialEq)]
enum ROITypes {
    Center,
    Corner,
}

#[derive(Debug, PartialEq)]
enum ColorSpaceOpt {
    Gray,
    Bayer,
    Rgb,
}

#[derive(Debug, PartialEq)]
enum CoolerStatus {
    On,
    Off,
}

impl Default for GenCamGUI {
    fn default() -> Self {
        
        Self {

            dialog_type: DialogType::Debug,
            modal_message: String::new(),
            dark_mode: false,

            modal_active: false,

            comms_stream: None,
            // comms_buffer: [0; 4096],
            // server_connection: false,

            // connected_cameras: HashMap::new(),

            data: None,
            image: None,
            img_uri: "image/png",

            frame: egui::Frame {
                inner_margin: 6.0.into(),
                outer_margin: 3.0.into(),
                rounding: 3.0.into(),
                shadow: egui::Shadow::NONE,
                //  {
                //     offset: [2.0, 3.0].into(),
                //     blur: 16.0,
                //     spread: 0.0,
                //     color: egui::Color32::from_black_alpha(245),
                // },
                fill: egui::Color32::from_white_alpha(0),
                stroke: egui::Stroke::new(1.0, egui::Color32::DARK_GRAY),
            },

            exposure_text_edit: "0.0".into(),
            long_exp_checkbox: false,
            exposure_slider: 0.0,
            auto_exp_checkbox: false,
            min_cam_temp: -80.0,
            max_cam_temp: 10.0,
            curr_cam_temp: 0.0,
            cooler_status: CoolerStatus::Off,
            color_space: ColorSpaceOpt::Gray,
            roi: [0.0, 0.0, 0.0, 0.0],
            roi_type: ROITypes::Center,
            roi_enabled: false,
            img_height: 0,
            img_width: 0,

            msg_list: CircularBuffer::new(),
            uri: "ws://localhost:9001".into(),
            ws: None,
            ctx: None,
        }
    }
}

impl GenCamGUI {    
    /// Instantiates an instance of a modal dialog window.
    fn dialog(&mut self, dialog_type: DialogType, message: &str) {
        match self.modal_active {
            true => {
                println!(
                    "A modal window is already active. The offending request was: [{}] {}",
                    dialog_type.as_str(),
                    message
                );
            }
            false => {
                self.modal_active = true;
                self.dialog_type = dialog_type;
                self.modal_message = message.to_owned();
            }
        }
    }

    /// Should be called each frame a dialog window needs to be shown.
    ///
    /// Should not be used to instantiate an instance of a dialog window, use `dialog()` instead.
    fn show_dialog(&mut self, ctx: &egui::Context) {
        self.modal_active = true;

        let title = self.dialog_type.as_str();

        egui::Window::new(title)
            .collapsible(false)
            .open(&mut self.modal_active)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let scale = 0.25;
                    match self.dialog_type {
                        DialogType::Debug => {
                            ui.add(
                                egui::Image::new(egui::include_image!(
                                    "../res/Gcg_Information.png"
                                ))
                                .fit_to_original_size(scale),
                            );
                        }
                        DialogType::Info => {
                            ui.add(
                                egui::Image::new(egui::include_image!(
                                    "../res/Gcg_Information.png"
                                ))
                                .fit_to_original_size(scale),
                            );
                        }
                        DialogType::Warn => {
                            ui.add(
                                egui::Image::new(egui::include_image!("../res/Gcg_Warning.png"))
                                    .fit_to_original_size(scale),
                            );
                        }
                        DialogType::Error => {
                            ui.add(
                                egui::Image::new(egui::include_image!("../res/Gcg_Error.png"))
                                    .fit_to_original_size(scale),
                            );
                        }
                    }
                    // ui.add(egui::Image::new(egui::include_image!(img_path)));
                    // ui.add(egui::Image::new(egui::include_image!(self.dialog_type.get_image_url())));

                    ui.vertical(|ui| {
                        // ui.add(egui::Label::new(self.modal_message.to_owned()).wrap(true));
                        ui.add(egui::Label::new(self.modal_message.to_owned()).wrap())
                    });
                });

                // if ui.button("Ok").clicked() {
                //     self.modal_active = false;
                // }
            });
    }

    // fn connect_to_server(&mut self) -> std::io::Result<()> {
    //     println!("Attempting connection to server...");
    //     let mut stream = TcpStream::connect("127.0.0.1:50042")?;
    //     let mut buffer = [0; 4096];

    //     let _ = stream.read(&mut buffer[..])?;
    //     println!(
    //         "Rxed Msg (Exp. Hello): {}",
    //         str::from_utf8(&buffer).unwrap()
    //     );

    //     // self.server_connection = true;

    //     self.comms_stream = Some(stream);

    //     Ok(())
    // }

    fn _receive_test_image(&mut self) -> std::io::Result<()> {
        self.msg_list.push_back("Attempting to receive image...".to_owned());
        let mut stream = self.comms_stream.as_ref().unwrap();
        let mut buffer = [0; 4096];
        
        self.msg_list.push_back("Writing to server.".to_owned());
        // Image test transfer.
        stream.write_all(b"SEND IMAGE TEST")?;
        let _ = stream.read(&mut buffer[..])?;
        println!(
            "Rxed Msg (Exp. SEND IMAGE TEST): {}",
            str::from_utf8(&buffer).unwrap()
        );

        // RX and deserialize...
        let rimg: GenericImageOwned = serde_json::from_str(
            str::from_utf8(&buffer)
                .unwrap()
                .trim_end_matches(char::from(0)),
        )
        .unwrap(); // Deserialize to generic image.
        println!("{:?}", rimg.get_metadata());
        println!("{:?}", rimg.get_image());
        let img: DynamicImage = rimg
            .get_image()
            .clone()
            .try_into()
            .expect("Could not convert image");

        let mut data = Cursor::new(Vec::new());
        img.write_to(&mut data, image::ImageFormat::Png).unwrap();
        self.data = Some(data.into_inner().into());
        self.image = Some(img);
        Ok(())
    }

    // Assuems the last binary data we received is a valid image (change this later).
    fn update_test_image(&mut self) -> std::io::Result<()> {
        // self.msg_list.push_back("Attempting to update image...".to_owned());
        // let mut stream = self.comms_stream.as_ref().unwrap();
        // let mut buffer = [0; 4096];
        
        // self.msg_list.push_back("Writing to server.".to_owned());
        // // Image test transfer.
        // stream.write_all(b"SEND IMAGE TEST")?;
        // let _ = stream.read(&mut buffer[..])?;
        // println!(
        //     "Rxed Msg (Exp. SEND IMAGE TEST): {}",
        //     str::from_utf8(&buffer).unwrap()
        // );

        // Cant update if we have no connection
        if self.ws.is_none() {
            return Ok(());
        }

        let ws = self.ws.as_mut().ok_or(std::io::Error::new(std::io::ErrorKind::Other, "Failed to get ws"))?;
        if let Some((data, img)) = ws.image_events.take() {
            self.data = Some(data.into());
            self.image = Some(img);
        }
        Ok(())
    }
    
    fn ui_developer_controls(&mut self, ctx: &egui::Context) {
        // Debug Controls Window for Developer Use Only
        egui::Window::new("Developer Controls").show(ctx, |ui| {
            // ui.heading("Developer Controls");
            ui.horizontal(|ui| {
                ui.label("Modal Controller:");
                if ui.button("Close").clicked() {
                    self.modal_active = false;
                }
                if ui.button("Debug").clicked() {
                    self.dialog(DialogType::Debug, "This is a debug message. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Etiam pharetra ex quis lacus efficitur luctus. Praesent sed lectus convallis, malesuada ex nec, pulvinar tortor. Pellentesque suscipit malesuada diam, sit amet lacinia nisi maximus in. Praesent mi tortor, pulvinar et pretium sed, maximus vitae nulla. Sed vitae nibh a ligula tempus rhoncus et ac mauris. Proin ipsum eros, aliquet quis sodales ac, egestas in mi. Curabitur est metus, sollicitudin in tincidunt ut, pulvinar eget turpis. Cras nec mattis quam, non ornare ipsum. Aliquam et viverra mauris, eget semper metus. Morbi imperdiet dui est, id posuere leo luctus imperdiet. ");
                }
                if ui.button("Info").clicked() {
                    self.dialog(DialogType::Info, "This is an informational message. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Etiam pharetra ex quis lacus efficitur luctus. Praesent sed lectus convallis, malesuada ex nec, pulvinar tortor. Pellentesque suscipit malesuada diam, sit amet lacinia nisi maximus in. Praesent mi tortor, pulvinar et pretium sed, maximus vitae nulla. Sed vitae nibh a lig");
                }
                if ui.button("Warn").clicked() {
                    self.dialog(DialogType::Warn, "This is a warning message. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Etiam pharetra ex quis lacus efficitur luctus. Praesent sed lectus convallis, malesuada ex nec, pulvinar tortor. Pe");
                }
                if ui.button("Error").clicked() {
                    self.dialog(DialogType::Error, "This is an error message.");
                }
            });

            ui.horizontal(|ui| {
                ui.label("Server Connection:");
                let mut disconnect = false;
                if let Some(ws) = &mut self.ws {
                    ui.horizontal(|ui| {
                        ui.label("WebSocket URI: ");
                        ui.label(&self.uri);
                        if ui.button("Disconnect").clicked() {
                            disconnect = true;
                        }
                    });
                    ws.ui(ui);
                } else {
                    ui.horizontal(|ui| {
                        ui.label("WebSocket URI: ");
                        ui.text_edit_singleline(&mut self.uri);
                        if ui.button("Connect").clicked() {
                            self.ws = WsBackend::connect(&self.uri, &self.ctx);
                        }
                    }); 
                }
                if disconnect {
                    self.ws.as_mut().unwrap().close(); // safe to unwrap since disconnect is only true if ws is Some
                    self.ws = None;
                }
            });
        });
    }


    fn ui_top_bar(&mut self, ctx: &egui::Context) {
        // Top Settings Panel
        egui::TopBottomPanel::top("top_panel")
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.modal_active, |ui| {
                    ui.horizontal(|ui| {
                        menu::bar(ui, |ui| {
                            ui.menu_button("File", |ui| {
                                if ui.button("Open").clicked() {
                                    // …
                                }
                            });
                            ui.menu_button("Edit", |ui| {
                                if ui.button("Open").clicked() {
                                    // …
                                }
                            });
                            ui.menu_button("View", |ui| match self.dark_mode {
                                true => {
                                    if ui.button("Switch to Light Mode").clicked() {
                                        ctx.set_visuals(Visuals::light());
                                        // ctx.set_visuals_of(egui::Theme::Light, Visuals::light());
                                        self.dark_mode = false;
                                    }
                                }
                                false => {
                                    if ui.button("Switch to Dark Mode").clicked() {
                                        ctx.set_visuals(Visuals::dark());
                                        // ctx.set_visuals_of(egui::Theme::Dark, Visuals::dark());
                                        self.dark_mode = true;
                                    }
                                }
                            });
                            ui.menu_button("About", |ui| {
                                if ui.button("Open").clicked() {
                                    // …
                                }
                            });
                            ui.menu_button("Help", |ui| {
                                if ui.button("Open").clicked() {
                                    // …
                                }
                            });
                        });
    
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                            ui.label(format!("v{}", env!("CARGO_PKG_VERSION")))
                        });
                    });
                });
            });
    }

    fn ui_left_panel(&mut self, ctx: &egui::Context, w_view: f32) {
        // Left Panel
        let w_scale = 0.5;

        egui::SidePanel::left("left_panel")
            .resizable(true)
            // .min_width(ctx.available_rect().width()/8.0)
            // .max_width(ctx.available_rect().width()/4.0)
            .width_range((w_view / (8.0 / w_scale))..=(w_view / (4.0 / w_scale)))
            .default_width(ctx.available_rect().width() / (6.0 / w_scale))
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.modal_active, |ui| {
                    ui.label("Window Controls");
                ui.separator(); // Placeholder to enable dragging (expands to fill).

                ui.label(format!(
                    "{:?}",
                    (w_view / (8.0 / w_scale))..=(w_view / (4.0 / w_scale))
                ));
                ui.label("Left Panel");
                });
            });
        }
        
        fn ui_right_panel(&mut self, ctx: &egui::Context, w_view: f32) {
            // Left Panel
            let w_scale = 1.0;
            
            egui::SidePanel::right("right_panel")
            .resizable(true)
            .width_range(w_view / (8.0 / w_scale)..=w_view / (2.0 / w_scale))
            .default_width(ctx.available_rect().width() / (6.0 / w_scale))
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    
                    self.frame.show(ui, |ui| {
                        egui::CollapsingHeader::new("Camera Controls")
                            .default_open(true)
                            .show(ui, |ui| {
                                ui.add_visible_ui(false, |ui| {
                                    ui.separator();
                                });
                                
                                ui.horizontal(|ui| {
                                    ui.label("Exposure");
                                });

                                ui.horizontal(|ui| {
                                    ui.add_enabled_ui(!self.auto_exp_checkbox, |ui| {
                                        ui.checkbox(&mut self.long_exp_checkbox, "LongExp");
                                    });
                                    ui.checkbox(&mut self.auto_exp_checkbox, "Auto");
                                });

                                ui.horizontal(|ui| {
                                    ui.add_enabled_ui(!self.auto_exp_checkbox, |ui| {
                                        ui.spacing_mut().slider_width = w_view / (6.0 / w_scale);
                                        if self.long_exp_checkbox {
                                            ui.add(egui::Slider::new(&mut self.exposure_slider, 0.5..=3600.0).suffix(" s"));
                                        } else {
                                            ui.add(egui::Slider::new(&mut self.exposure_slider, 0.0..=5000.0).suffix(" ms"));
                                        }
                                    });
                                });

                                ui.horizontal(|ui| {
                                    // ui.label("Options");
                                    if ui.button("Options").clicked() {
                                        // Open options menu.
                                    }
                                });

                                // ui.label("This is a collapsible section.");
                                // if ui
                                //     .button("Acquire Image")
                                //     .on_hover_text("Acquire an image from the camera.")
                                //     .clicked()
                                // {
                                //     // Acquire image.
                                //     self.receive_test_image().expect("Failed to receive image.");
                                // }

                            });
                    });

                    self.frame.show(ui, |ui| {
                        egui::CollapsingHeader::new("Thermal Controls")
                            .default_open(true)
                            .show(ui, |ui| {
                                ui.add_visible_ui(false, |ui| {
                                    ui.separator();
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Temperature");
                                    ui.label(format!("{} °C", self.curr_cam_temp));
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Cooler");
                                    egui::ComboBox::from_id_source("CoolerStatus")
                                        .selected_text(format!("{:?}", self.cooler_status))
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut self.cooler_status, CoolerStatus::On, "On");
                                            ui.selectable_value(&mut self.cooler_status, CoolerStatus::Off, "Off");
                                    });
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Target");
                                    ui.add(egui::Slider::new(&mut self.exposure_slider, self.min_cam_temp..=self.max_cam_temp).suffix(" °C"));
                                });
                                
                            });
                    });

                    self.frame.show(ui, |ui| {
                        egui::CollapsingHeader::new("Image Controls")
                            .default_open(true)
                            .show(ui, |ui| {
                                ui.add_visible_ui(false, |ui| {
                                    ui.separator();
                                });
                                ui.label("This is a collapsible section.");
                                if ui
                                    .button("Get Exposure")
                                    .on_hover_text("On hover text TBD.")
                                    .clicked()
                                {
                                    // Get exposure value.
                                }
                                if ui
                                    .button("Set Exposure")
                                    .on_hover_text("On hover text TBD.")
                                    .clicked()
                                {
                                    // Set exposure value.
                                }
                                ui.checkbox(&mut true, "Enable Auto-Exposure");
                            });
                    });

                    self.frame.show(ui, |ui| {
                        egui::CollapsingHeader::new("File Saving")
                            .default_open(true)
                            .show(ui, |ui| {
                                ui.add_visible_ui(false, |ui| {
                                    ui.separator();
                                });

                                if ui.button("Browse").clicked() {
                                    // Open file dialog.
                                }
                            });
                    });

                    self.frame.show(ui, |ui| {
                        egui::CollapsingHeader::new("Capture Format and Area")
                            .default_open(true)
                            .show(ui, |ui| {
                                ui.add_visible_ui(false, |ui| {
                                    ui.separator();
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Output Format");
                                    egui::ComboBox::from_id_source("CoolerStatus")
                                        .selected_text(format!("{:?}", self.cooler_status))
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut self.cooler_status, CoolerStatus::On, "On");
                                            ui.selectable_value(&mut self.cooler_status, CoolerStatus::Off, "Off");
                                    });
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Color Space");
                                    egui::ComboBox::from_id_source("ColorSpace")
                                        .selected_text(format!("{:?}", self.color_space))
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut self.color_space, ColorSpaceOpt::Gray, "Grayscale");
                                            ui.selectable_value(&mut self.color_space, ColorSpaceOpt::Bayer, "Bayer");
                                            ui.selectable_value(&mut self.color_space, ColorSpaceOpt::Rgb, "RGB");
                                    });
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Capture ROI");
                                    ui.checkbox(&mut self.roi_enabled, "Use ROI");
                                    egui::ComboBox::from_id_source("ColorSpace")
                                        .selected_text(format!("{:?}", self.roi_type))
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut self.roi_type, ROITypes::Center, "Centered");
                                            ui.selectable_value(&mut self.roi_type, ROITypes::Corner, "Corner");
                                    });
                                });

                                ui.horizontal(|ui| {
                                    ui.label("ROI");
                                    ui.label("Centered");
                                    ui.add(egui::Slider::new(&mut self.roi[0], 0.0..=self.img_width as f32).suffix("X"));
                                    ui.add(egui::Slider::new(&mut self.roi[1], 0.0..=self.img_height as f32).suffix("Y"));
                                    ui.add(egui::Slider::new(&mut self.roi[2], 0.0..=100.0).suffix("L"));
                                    ui.add(egui::Slider::new(&mut self.roi[3], 0.0..=100.0).suffix("W"));
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Binning");
                                    egui::ComboBox::from_id_source("CoolerStatus")
                                        .selected_text(format!("{:?}", self.cooler_status))
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut self.cooler_status, CoolerStatus::On, "On");
                                            ui.selectable_value(&mut self.cooler_status, CoolerStatus::Off, "Off");
                                    });
                                });
                            });
                    });

                });

                ui.add_enabled_ui(!self.modal_active, |ui| {
                    ui.label("Communication Log");
                    ui.separator(); // Placeholder to enable dragging (expands to fill).

                    if ui.button("Generate New Msg").clicked() {
                        self.msg_list
                            .push_back(format!("Hello! This is message #{}. This is a long message because it contains a lot of data!", self.msg_list.len()));
                    }

                    egui::ScrollArea::both()
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                    
                            ui.label(format!(
                                "{:?}",
                                (w_view / (8.0 / w_scale))..=(w_view / (4.0 / w_scale))
                            ));

                            ui.label("Messages");
                            for msg in self.msg_list.iter() {
                                ui.add(egui::Label::new(msg).truncate());
                            }

                            ui.label("Events");

                            match &mut self.ws {
                                None => {
                                    ui.label("No websocket connection.");
                                }
                                Some(ws) => {
                                    for event in ws.events.iter() {   
                                        ui.add(egui::Label::new(format!("{:?}", event)).truncate());
                                        // match event {
                                        //     WsEvent::Message(WsMessage::Binary(data)) => {
                                        //         let pkt: GenCamPacket = serde_json::from_slice(data).expect("Failed to deserialize packet.");
                                        //         ui.add(egui::Label::new(format!("{:?}", pkt)).truncate());
                                        //     }
                                        //     _ => {
                                        //     }
                                        // }
                                    }
                                }
                            }
                        });
                });
            });
    }

    fn ui_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // ui.label("Test.");
            // ui.label(format!("Avail   {:?}", ctx.available_rect()));
            // ui.label(format!("Used    {:?}", ctx.used_rect()));
            // ui.label(format!("Screen  {:?}", ctx.screen_rect()));

            // let winsize = ui.ctx().input(|i: &egui::InputState| i.screen_rect());
            // let win_width = winsize.width();
            // let win_height = winsize.height();

            // ui.label(format!(
            //     "The window size is: {} x {}",
            //     win_width, win_height
            // ));

            ui.vertical(|ui| {
                // Here we show the image data.
                self.frame.show(ui, |ui| {

                    if let Some(data) = self.data.clone() {
                        ui.add(
                            egui::Image::new(ImageSource::Bytes {
                                uri: self.img_uri.into(),
                                bytes: data,
                            })
                            .rounding(10.0)
                            // .fit_to_original_size(1.0),
                        );
                    }
                });

                self.frame.show(ui, |ui| {
                    ui.label("Image Controls");

                    ui.horizontal_wrapped(|ui: &mut egui::Ui| {
                        // Examples / tests on on-the-fly image manipulation.
                        // Button
                        if ui
                            .button("Swap Image")
                            .on_hover_text("Swap the image data.")
                            .clicked()
                        {
                            self.update_test_image().expect("Failed to update image.");
                        }

                        if ui
                            .button("Reload Image")
                            .on_hover_text("Refresh the image to reflect changed data.")
                            .clicked()
                        {
                            ui.ctx().forget_image(self.img_uri);
                        }

                        if ui
                            .button("Nuke Image")
                            .on_hover_text("Set all bytes to 0x0.")
                            .clicked()
                        {
                            // Change all values in self.data to 0.
                            self.data.take();

                            ui.ctx().forget_image(self.img_uri);
                        }

                    });
                });
            });

            // ui.columns(2, |col| {
            //     // When inside a layout closure within the column we can just use 'ui'.

            //     // FIRST COLUMN
            //     col[0].label("First column");
            //     col[0].vertical(|ui| {
            //         // Here we show the image data.
            //         self.frame.show(ui, |ui| {
            //             if let Some(data) = &self.data {
            //                 ui.add(
            //                     egui::Image::new(ImageSource::Bytes {
            //                         uri: self.img_uri.clone().into(),
            //                         bytes: data.clone(),
            //                     })
            //                     .rounding(10.0)
            //                     // .fit_to_original_size(1.0),
            //                 );
            //             } else {
            //                 ui.label("No image data.");
            //             }
            //         });

            //         self.frame.show(ui, |ui| {
            //             ui.label("Image Controls");

            //             ui.horizontal_wrapped(|ui: &mut egui::Ui| {
            //                 // Examples / tests on on-the-fly image manipulation.
            //                 // Button
            //                 if ui
            //                     .button("Swap Image")
            //                     .on_hover_text("Swap the image data.")
            //                     .clicked()
            //                 {
            //                     self.update_test_image().expect("Failed to update image.");
            //                 }

            //                 if ui
            //                     .button("Reload Image")
            //                     .on_hover_text("Refresh the image to reflect changed data.")
            //                     .clicked()
            //                 {
            //                     ui.ctx().forget_image(&self.img_uri.clone());
            //                 }

            //                 if ui
            //                     .button("Nuke Image")
            //                     .on_hover_text("Set all bytes to 0x0.")
            //                     .clicked()
            //                 {
            //                     // Change all values in self.data to 0.
            //                     self.data.take();

            //                     ui.ctx().forget_image(&self.img_uri.clone());
            //                 }
            //             });
            //         });
            //     });

            //     // SECOND COLUMN
            //     col[1].label("Second column");
            //     col[1].vertical(|ui| {
            //         self.frame.show(ui, |ui| {
            //             ui.collapsing("GenCam Controls 1", |ui| {
            //                 ui.label("This is a collapsible section.");
            //                 if ui
            //                     .button("Acquire Image")
            //                     .on_hover_text("Acquire an image from the camera.")
            //                     .clicked()
            //                 {
            //                     // Acquire image.
            //                     self.receive_test_image().expect("Failed to receive image.");
            //                 }
            //             });
            //         });

            //         self.frame.show(ui, |ui| {
            //             ui.collapsing("GenCam Controls 2", |ui| {
            //                 ui.label("This is a collapsible section.");
            //             });
            //         });

            //         self.frame.show(ui, |ui| {
            //             ui.collapsing("Non-GenCam Controls", |ui| {
            //                 ui.label("This is a collapsible section.");
            //                 if ui
            //                     .button("Get Exposure")
            //                     .on_hover_text("On hover text TBD.")
            //                     .clicked()
            //                 {
            //                     // Get exposure value.
            //                 }
            //                 if ui
            //                     .button("Set Exposure")
            //                     .on_hover_text("On hover text TBD.")
            //                     .clicked()
            //                 {
            //                     // Set exposure value.
            //                 }
            //                 ui.checkbox(&mut true, "Enable Auto-Exposure");
            //             });
            //         });

            //         self.frame.show(ui, |ui| {
            //             ui.collapsing("File Saving", |ui| {
            //                 ui.label("This is a collapsible section.");
            //             });
            //         });
            //     });
            // });

        if let Some(data) = &self.data {
            ui.label(format!("{}", data.len()));
        } else {
            ui.label("No image data.");
        }

        // Image controls
        egui::Frame::default()
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .rounding(ui.visuals().widgets.noninteractive.rounding)
            .inner_margin(3.0)
            .outer_margin(3.0)
            // .shadow(egui::Shadow::new([8.0, 12.0].into(), 16.0, egui::Color32::from_black_alpha(180)))
            .show(ui, |ui| {
                ui.label("Test text!");
            });
        });
    }

    fn ui_bottom_bar(&mut self, ctx: &egui::Context) {
        // Bottom Status Panel
        egui::TopBottomPanel::bottom("bottom_panel")
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.modal_active, |ui| ui.horizontal(|ui| ui.label("Bottom Status Panel")));
                // ui.set_enabled(!self.modal_active);
                // ui.horizontal(|ui| {
                //     ui.label("Bottom Status Panel");
                // });
            });
    }
}

impl eframe::App for GenCamGUI {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_pixels_per_point(1.5);

        ctx.request_repaint();

        //////////////////////////////////////////////////////////////
        // All possible modal window popups should be handled here. //
        //////////////////////////////////////////////////////////////

        // There should only ever be one modal window active, and it should be akin to a dialog window - info, warn, or error.

        if self.modal_active {
            self.show_dialog(ctx);
        }

        //////////////////////////////////////////////////////////////
        //////////////////////////////////////////////////////////////
        //////////////////////////////////////////////////////////////

        let w_view = ctx.screen_rect().width();
        let mut pack = true;
        if let Some(mut ws) = self.ws.take() {
            // Push any event to either the general event vector or the image vector.
            while let Some(event) = ws.ws_receiver.try_recv() {
                match event {
                    WsEvent::Message(WsMessage::Binary(ref data)) => { // All messages should be binary
                        let pkt: Packet = bincode::deserialize(data).expect("Failed to deserialize packet.");
                        match pkt {
                            Packet::Response(_, _, img) => {
                                let img = img.unwrap(); // Unwrap the image, we know it is an image
                                match img {
                                    GenSrvValue::Image(img) => {
                                        let dimg: DynamicImage = img.try_into().unwrap();
                                        let mut buf = Cursor::new(Vec::new());
                                        dimg.write_to(&mut buf, image::ImageFormat::Png).unwrap();
                                        ws.image_events = Some((buf.into_inner(), dimg));
                                        ws.new_image_event.store(true, std::sync::atomic::Ordering::Relaxed);
                                    }
                                    _ => {
                                        ws.events.push(event);
                                    }
                                }
                            }
                            _ => {
                                ws.events.push(event);
                            }
                        }
                    }
                    WsEvent::Closed => {
                        ws.close();
                        pack = false;
                        break;
                    }
                    _ => {
                        ws.events.push(event);
                    }
                }
            }
            if pack {
                self.ws = Some(ws);
            }
        }

        // if self.ws.is_some() && self.ws.as_ref().unwrap().new_image_event.swap(false, std::sync::atomic::Ordering::Relaxed) {
        //     self.update_test_image().unwrap();
        //     ctx.forget_image(&self.img_uri);
        //     ctx.request_repaint(); // May not be able to keep this if we get spammed w/ images.
        // }

        if let Some(ws) = &mut self.ws {
            if ws.new_image_event.swap(false, std::sync::atomic::Ordering::Relaxed) {
                self.update_test_image().unwrap();
                ctx.forget_image(self.img_uri);
                ctx.request_repaint(); // May not be able to keep this if we get spammed w/ images.
            }
        }

        self.ui_developer_controls(ctx);
        self.ui_top_bar(ctx);
        self.ui_left_panel(ctx, w_view);
        self.ui_right_panel(ctx, w_view);
        self.ui_bottom_bar(ctx);
        self.ui_central_panel(ctx);
    }
}

// fn execute<F: Future<Output = ()> + 'static>(f: F) {
//     wasm_bindgen_futures::spawn_local(f);
// }