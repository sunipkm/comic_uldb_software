use std::collections::HashMap;
use generic_camera::{server::{GenSrvCmd, GenSrvOutput}, GenCamDescriptor};
use serde::{Deserialize, Serialize};

#[non_exhaustive]
#[derive(Debug, Serialize, Deserialize)]
pub enum Packet {
    Ack,
    Nack,
    Enumerate,
    Disconnect(u32),
    Command(u32, u32, GenSrvCmd),
    CameraList(HashMap<u32, GenCamDescriptor>),
    Response(u32, u32, GenSrvOutput),
}