use asolaria_native_dock::{config::Config,serve};
fn main() {
    let args:Vec<String>=std::env::args().skip(1).collect();
    if args==["--help"] {
        println!("asolaria-native-dock --station ACER|FALCON --bind 127.0.0.1:5090 --peer 127.0.0.1:PEER_PORT --key-file LOCAL_FILE [--rooms EXISTING_ABSOLUTE_ROOT]");
        return;
    }
    let result=Config::parse(args).and_then(serve);
    if let Err(code)=result {
        eprintln!("DOCKERROR|code={code}|room_writes=0|json=0");
        std::process::exit(1);
    }
}
