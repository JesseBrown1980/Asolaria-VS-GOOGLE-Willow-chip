use std::{fs::File, io::Read, net::{IpAddr, Ipv4Addr, SocketAddr}, path::PathBuf};

pub const SCHEMA: &str = "ASOLARIA-NATIVE-DOCK-V1";
pub const MAX_WORKERS: usize = 16;
pub const MAX_COMMAND: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Station { Acer, Falcon }
impl Station {
    pub fn parse(s: &str) -> Result<Self, &'static str> {
        match s { "ACER" => Ok(Self::Acer), "FALCON" => Ok(Self::Falcon), _ => Err("station_must_be_ACER_or_FALCON") }
    }
    pub fn name(self) -> &'static str { match self { Self::Acer => "ACER", Self::Falcon => "FALCON" } }
}

#[derive(Clone)]
pub struct Key([u8;64]);
impl Key {
    pub fn parse(text: &[u8]) -> Result<Self, &'static str> {
        let text = std::str::from_utf8(text).map_err(|_| "key_shape")?.trim();
        if text.len()!=64 || !text.bytes().all(|b| b.is_ascii_hexdigit()) { return Err("key_shape"); }
        let mut bytes=[0;64]; bytes.copy_from_slice(text.as_bytes()); Ok(Self(bytes))
    }
    pub fn matches(&self, text: &[u8]) -> bool {
        // Fixed 64 iterations, including a length mismatch. No early exit on a differing byte.
        let mut difference = text.len() ^ self.0.len();
        for (i, expected) in self.0.iter().enumerate() {
            difference |= usize::from(*expected ^ text.get(i).copied().unwrap_or(0));
        }
        difference == 0
    }
    pub(crate) fn wire(&self) -> &str {
        // The constructor permits ASCII hex only.
        std::str::from_utf8(&self.0).expect("validated ASCII key")
    }
}

#[derive(Clone)]
pub struct Config {
    pub station: Station,
    pub bind: SocketAddr,
    pub peer: SocketAddr,
    pub rooms: Option<PathBuf>,
    pub(crate) key: Key,
}
pub fn loopback_addr(s: &str) -> Result<SocketAddr, &'static str> {
    let addr: SocketAddr=s.parse().map_err(|_| "socket_address")?;
    if addr.ip()!=IpAddr::V4(Ipv4Addr::LOCALHOST) || addr.port()==0 { return Err("loopback_127_0_0_1_required"); }
    Ok(addr)
}
impl Config {
    pub fn parse(args: impl IntoIterator<Item=String>) -> Result<Self, &'static str> {
        let mut station=None; let mut bind=None; let mut peer=None; let mut key_file=None; let mut rooms=None;
        let mut args=args.into_iter();
        while let Some(flag)=args.next() {
            let value=args.next().ok_or("argument_value_missing")?;
            match flag.as_str() {
                "--station" if station.is_none()=>station=Some(Station::parse(&value)?),
                "--bind" if bind.is_none()=>bind=Some(loopback_addr(&value)?),
                "--peer" if peer.is_none()=>peer=Some(loopback_addr(&value)?),
                "--key-file" if key_file.is_none()=>key_file=Some(PathBuf::from(value)),
                "--rooms" if rooms.is_none()=>rooms=Some(PathBuf::from(value)),
                _=>return Err("unknown_or_duplicate_argument"),
            }
        }
        let station=station.ok_or("station_required")?;
        let bind=bind.ok_or("bind_required")?;
        let peer=peer.ok_or("peer_required")?;
        if bind==peer { return Err("peer_is_local_listener"); }
        if let Some(path)=&rooms {
            if !path.is_absolute() || !path.is_dir() { return Err("rooms_absolute_existing_directory_required"); }
        }
        let mut bytes=Vec::new();
        File::open(key_file.ok_or("key_file_required")?).map_err(|_| "key_file_open")?
            .take(129).read_to_end(&mut bytes).map_err(|_| "key_file_read")?;
        if bytes.len()>128 { return Err("key_shape"); }
        Ok(Self { station, bind, peer, rooms, key:Key::parse(&bytes)? })
    }
}
