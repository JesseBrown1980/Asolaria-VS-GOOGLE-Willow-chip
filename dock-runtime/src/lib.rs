//! Loopback-only HBP federation. Stations keep their identities and address
//! spaces separate; a room RPC reads eight existing bytes, not coherent RAM.
#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod config;
mod http;

use config::{Config,Station,MAX_COMMAND,MAX_WORKERS,SCHEMA};
use std::{
    fs::File,
    io::Read,
    net::{TcpListener,TcpStream},
    sync::{atomic::{AtomicUsize,Ordering},Arc},
    thread,
};

pub struct Reply { pub status:u16, pub body:String }
fn ok(body:String)->Reply { Reply {status:200,body} }
fn error(status:u16,code:&str)->Reply { Reply {status,body:format!("DOCKERROR|code={code}|json=0\n")} }

fn resource_row()->String {
    let measured=(|| {
        let mut text=String::new();
        File::open("/proc/meminfo").ok()?.take(16385).read_to_string(&mut text).ok()?;
        if text.len()>16384 { return None; }
        let mut total=None;let mut available=None;
        for line in text.lines() {
            let (name,rest)=line.split_once(':')?;
            if name=="MemTotal" || name=="MemAvailable" {
                let mut words=rest.split_ascii_whitespace();
                let kib=words.next()?.parse::<u64>().ok()?;
                if words.next()!=Some("kB") || words.next().is_some() { return None; }
                let bytes=kib.checked_mul(1024)?;
                if name=="MemTotal" { total=Some(bytes); } else { available=Some(bytes); }
            }
        }
        Some((total?,available?))
    })();
    match measured {
        Some((total,available))=>format!("DOCKRESOURCE|source=proc_meminfo|physical_ram_total_bytes={total}|physical_ram_available_bytes={available}|scope=local_device_only|shared_physical_memory=0|json=0\n"),
        None=>String::from("DOCKRESOURCE|source=NOT_MEASURED|physical_ram_total_bytes=NOT_MEASURED|physical_ram_available_bytes=NOT_MEASURED|scope=local_device_only|shared_physical_memory=0|json=0\n"),
    }
}
pub fn station_rows(config:&Config)->String {
    let rooms=if config.rooms.is_some() {"CONFIGURED"} else {"NOT_MEASURED"};
    format!("DOCKSTATION|schema={SCHEMA}|station={}|station_label_is_device_pid=0|device_identity=owner_registry_preserved|room_source={rooms}|room_access=read_only_existing_identity_plane|tuple_dim=60|axes=preserved|process_per_room=0|json=0\n{}",
        config.station.name(),resource_row())
}

/// Inspect the same bounded pure command vocabulary used by the shared core.
/// This does not execute a command and is also used before a peer RPC.
pub fn inspect_command(command:&str)->Result<(),&'static str> {
    if command.is_empty() || command.len()>MAX_COMMAND || command.bytes().any(|b| !(32..127).contains(&b) || b==b'|') { return Err("command_shape"); }
    let command=command.trim();
    let (verb,arg)=command.split_once(' ').unwrap_or((command,""));let arg=arg.trim();
    match verb {
        "status"|"memory" if arg.is_empty()=>Ok(()),
        "room"=> {
            let n=arg.parse::<u64>().map_err(|_| "room_number")?;
            asolaria_unified_core::address(n).map(|_|())
        }
        "group"=>asolaria_unified_core::group_start_room(arg).map(|_|()),
        "sha" if arg.len()%2==0 && arg.bytes().all(|b|b.is_ascii_hexdigit())=>Ok(()),
        "gnn"=> {
            let values:Vec<i64>=arg.split(',').map(|v|v.parse().map_err(|_|"feature_integer")).collect::<Result<_,_>>()?;
            if values.len()!=12 || values.iter().any(|v|v.unsigned_abs()>(1<<40)) {return Err("twelve_bounded_Q24_features_required");} Ok(())
        }
        "prism"=> {
            let values:Vec<u64>=arg.split(',').map(|v|v.parse().map_err(|_|"lane_integer")).collect::<Result<_,_>>()?;
            if values.len()!=27 || values.iter().any(|&v|v>=1_000_081) {return Err("twenty_seven_bounded_lanes_required");} Ok(())
        }
        _=>Err("command_not_allowlisted"),
    }
}
fn local_task(config:&Config,command:&str)->Reply {
    if let Err(e)=inspect_command(command) {return error(400,e);}
    let verb=command.trim().split(' ').next().unwrap_or("");
    let result=if matches!(verb,"room"|"group"|"memory") {
        if config.rooms.is_none() {return error(503,"room_source_NOT_MEASURED");}
        asolaria_unified_core::native::with_rooms(|r| asolaria_unified_core::run(command,Some(r)))
    } else {asolaria_unified_core::run(command,None)};
    match result {Ok(body)=>ok(body),Err(e)=>error(400,e)}
}
fn room_number(query:&str)->Result<u64,&'static str> {
    let text=http::single_query(query,"n")?;
    if text.is_empty() || !text.bytes().all(|b|b.is_ascii_digit()) {return Err("room_number");}
    let n=text.parse::<u64>().map_err(|_|"room_number")?;
    asolaria_unified_core::address(n)?;Ok(n)
}
fn group_selector(query:&str)->Result<String,&'static str> {
    let text=http::single_query(query,"g")?;
    asolaria_unified_core::group_start_room(&text)?;
    Ok(text)
}
fn peer_station(config:&Config)->Result<(Station,String),&'static str> {
    let body=http::fetch(config.peer,config.key.wire(),"/station.hbp")?;
    let first=body.lines().next().ok_or("peer_station_missing")?;
    if first.split('|').next()!=Some("DOCKSTATION") {return Err("peer_station_schema");}
    let mut schema=None;let mut station=None;
    for field in first.split('|').skip(1) {
        if let Some(value)=field.strip_prefix("schema=") {
            if schema.replace(value).is_some() {return Err("peer_station_schema");}
        }
        if let Some(value)=field.strip_prefix("station=") {
            if station.replace(Station::parse(value)?).is_some() {return Err("peer_station_schema");}
        }
    }
    if schema!=Some(SCHEMA) {return Err("peer_station_schema");}
    let station=station.ok_or("peer_station_missing")?;
    if station==config.station {return Err("peer_station_loop_refused");}
    Ok((station,body))
}
fn peer_task(config:&Config,command:&str)->Reply {
    if let Err(e)=inspect_command(command) {return error(400,e);}
    if let Err(e)=peer_station(config) {return error(503,e);}
    match http::fetch(config.peer,config.key.wire(),&format!("/task.hbp?cmd={}",http::encode(command))) {
        Ok(body)=>ok(body),Err(e)=>error(503,e),
    }
}
fn route(config:&Config,request:http::Request)->Reply {
    if request.path=="/health.hbp" && request.query.is_empty() {
        return ok(format!("DOCKHEALTH|schema={SCHEMA}|service=native_dock|state=RESPONDING|auth_details=private|json=0\n"));
    }
    if !request.bearer.as_ref().is_some_and(|s|config.key.matches(s.as_bytes())) {return error(401,"authentication_required");}
    match request.path.as_str() {
        "/station.hbp" if request.query.is_empty()=>ok(station_rows(config)),
        "/dock.hbp" if request.query.is_empty()=>match peer_station(config) {
            Ok((peer,body))=>ok(format!("DOCKLINK|schema={SCHEMA}|local_station={}|peer_station={}|transport=fixed_loopback_peer|memory_model=independent_stations_read_only_RPC|coherent_shared_address_space=0|tuple_dim=60|axes=preserved|runtime_provider_calls=0|runtime_gpu_calls=0|process_per_room=0|room_writes=0|json=0\nDOCKLOCAL|station={}|json=0\n{}DOCKPEER|station={}|json=0\n{body}",
                config.station.name(),peer.name(),config.station.name(),station_rows(config),peer.name())),
            Err(e)=>error(503,e),
        },
        "/memory/room.hbp"=>match room_number(&request.query) {
            Ok(n)=>local_task(config,&format!("room {n}")),Err(e)=>error(400,e),
        },
        "/peer/room.hbp"=>match room_number(&request.query) {
            Ok(n)=>peer_task(config,&format!("room {n}")),Err(e)=>error(400,e),
        },
        "/memory/group.hbp"=>match group_selector(&request.query) {
            Ok(g)=>local_task(config,&format!("group {g}")),Err(e)=>error(400,e),
        },
        "/peer/group.hbp"=>match group_selector(&request.query) {
            Ok(g)=>peer_task(config,&format!("group {g}")),Err(e)=>error(400,e),
        },
        "/task.hbp"=>match http::single_query(&request.query,"cmd") {
            Ok(command)=>local_task(config,&command),Err(e)=>error(400,e),
        },
        "/peer/task.hbp"=>match http::single_query(&request.query,"cmd") {
            Ok(command)=>peer_task(config,&command),Err(e)=>error(400,e),
        },
        _=>error(404,"route_not_found"),
    }
}
fn connection(mut stream:TcpStream,config:&Config) {
    let reply=match http::request(&mut stream) {Ok(request)=>route(config,request),Err(e)=>error(400,e)};
    let _=http::send(&mut stream,reply.status,&reply.body);
}
struct Worker(Arc<AtomicUsize>);
impl Drop for Worker {
    fn drop(&mut self) {self.0.fetch_sub(1,Ordering::SeqCst);}
}
pub fn serve(config:Config)->Result<(),&'static str> {
    if let Some(rooms)=&config.rooms {std::env::set_var("ASOLARIA_ROOMS",rooms);}
    let listener=TcpListener::bind(config.bind).map_err(|_|"bind_failed")?;
    println!("DOCKSTART|schema={SCHEMA}|station={}|bind={}|peer={}|workers_max={MAX_WORKERS}|runtime=Rust_1_81|float_used=0|json_hot_path=0|node_v8=0|room_writes=0|json=0",config.station.name(),config.bind,config.peer);
    let config=Arc::new(config);let active=Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(mut stream)=stream else {continue;};
        if !stream.peer_addr().is_ok_and(|addr|addr.ip().is_loopback()) {continue;}
        if active.fetch_add(1,Ordering::SeqCst)>=MAX_WORKERS {
            active.fetch_sub(1,Ordering::SeqCst);
            let _=stream.set_write_timeout(Some(http::IO_TIMEOUT));
            let _=http::send(&mut stream,503,"DOCKERROR|code=worker_limit|json=0\n");
            continue;
        }
        let worker=Worker(Arc::clone(&active));let config=Arc::clone(&config);
        // Dropping an unspawned closure also drops Worker, releasing its slot.
        let _=thread::Builder::new().name("native-dock".into()).stack_size(256*1024).spawn(move||{
            let _worker=worker;connection(stream,&config);
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::Key;
    fn config(station:Station,peer:std::net::SocketAddr)->Config {
        Config {station,bind:"127.0.0.1:5090".parse().unwrap(),peer,rooms:None,key:Key::parse(&[b'a';64]).unwrap()}
    }
    fn req(path:&str,key:Option<&str>)->http::Request {
        let (path,query)=path.split_once('?').unwrap_or((path,""));
        http::Request {path:path.into(),query:query.into(),bearer:key.map(str::to_owned)}
    }
    #[test]
    fn private_routes_require_the_exact_key() {
        let c=config(Station::Acer,"127.0.0.1:5091".parse().unwrap());
        for path in ["/station.hbp","/dock.hbp","/task.hbp?cmd=status","/peer/room.hbp?n=0","/memory/group.hbp?g=omega"] {
            assert_eq!(route(&c,req(path,None)).status,401);
            assert_eq!(route(&c,req(path,Some(&"b".repeat(64)))).status,401);
        }
        assert_eq!(route(&c,req("/health.hbp",None)).status,200);
        assert_eq!(route(&c,req("/station.hbp",Some(&"a".repeat(64)))).status,200);
        assert!(!c.key.matches(&[b'a';63])); assert!(!c.key.matches(&[b'a';65]));
    }
    #[test]
    fn refuses_shell_paths_injection_and_invalid_math_before_peer_connect() {
        for bad in ["execute whoami","run shell","roomaddr 1","room 12500000000","room -1","group 462962962","group -1","memory x","status|json=1","status\n","sha gg","gnn 0,0","prism 1,2"] {
            assert!(inspect_command(bad).is_err(),"{bad}");
        }
        assert!(inspect_command("group 0").is_ok());
        assert!(inspect_command("group omega").is_ok());
        let c=config(Station::Acer,"127.0.0.1:1".parse().unwrap());
        assert_eq!(peer_task(&c,"execute whoami").status,400);
        assert_eq!(local_task(&c,"room 0").status,503);
        assert_eq!(local_task(&c,"group omega").status,503);
        assert!(station_rows(&c).contains("room_source=NOT_MEASURED"));
        assert_eq!(local_task(&c,"sha 616263").body,asolaria_unified_core::run("sha 616263",None).unwrap());
    }
    #[test]
    fn request_boundary_refuses_remote_authorities_duplicates_and_bodies() {
        let valid=b"GET /health.hbp HTTP/1.1\r\nHost: 127.0.0.1:15091\r\n\r\n";
        assert!(http::parse_request(valid).is_ok());
        for bad in [
            "GET / HTTP/1.1\r\nHost: remote.example\r\n\r\n",
            "GET / HTTP/1.1\r\nHost: localhost\r\nOrigin: http://evil.example\r\n\r\n",
            "GET / HTTP/1.1\r\nHost: localhost\r\nHost: localhost\r\n\r\n",
            "GET / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 1\r\n\r\n",
            "POST / HTTP/1.1\r\nHost: localhost\r\n\r\n",
        ] {assert!(http::parse_request(bad.as_bytes()).is_err(),"{bad}");}
        assert!(http::parse_request(&vec![b'x';http::MAX_HEADER+1]).is_err());
        assert!(http::single_query("cmd=status%0A","cmd").is_err());
        assert!(http::single_query("cmd=status&cmd=memory","cmd").is_err());
    }
    #[test]
    fn peer_identity_loop_is_refused_and_different_station_is_composed() {
        for (station,expected) in [("ACER",503),("FALCON",200)] {
            let listener=TcpListener::bind("127.0.0.1:0").unwrap();
            let addr=listener.local_addr().unwrap();
            let station=station.to_owned();
            let worker=thread::spawn(move|| {
                let (mut stream,_)=listener.accept().unwrap();
                let request=http::request(&mut stream).unwrap();
                assert_eq!(request.path,"/station.hbp");
                assert_eq!(request.bearer,Some("a".repeat(64)));
                http::send(&mut stream,200,&format!("DOCKSTATION|schema={SCHEMA}|station={station}|json=0\n")).unwrap();
            });
            let reply=route(&config(Station::Acer,addr),req("/dock.hbp",Some(&"a".repeat(64))));
            assert_eq!(reply.status,expected);
            if expected==200 {assert!(reply.body.contains("coherent_shared_address_space=0"));}
            else {assert!(reply.body.contains("peer_station_loop_refused"));}
            worker.join().unwrap();
        }
    }
    #[test]
    fn authenticated_local_task_over_real_tcp_uses_shared_integer_core() {
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();
        let addr=listener.local_addr().unwrap();
        let c=config(Station::Falcon,"127.0.0.1:1".parse().unwrap());
        let worker=thread::spawn(move|| {
            let (stream,_)=listener.accept().unwrap();connection(stream,&c);
        });
        let body=http::fetch(addr,&"a".repeat(64),"/task.hbp?cmd=sha%20616263").unwrap();
        assert!(body.contains("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
        worker.join().unwrap();
    }
    #[test]
    fn cli_socket_and_key_boundaries() {
        assert!(config::loopback_addr("0.0.0.0:5090").is_err());
        assert!(config::loopback_addr("192.168.1.2:5090").is_err());
        assert!(config::loopback_addr("127.0.0.1:0").is_err());
        assert!(Key::parse(&[b'g';64]).is_err());
        assert!(Key::parse(&[b'a';63]).is_err());
        assert!(Config::parse(["--station","ACER","--bind","127.0.0.1:5090","--peer","127.0.0.1:5090","--key-file","unused"].map(String::from)).is_err());
    }
}
