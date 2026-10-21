use std::{io::{Read,Write}, net::TcpStream, time::Duration};

pub const MAX_HEADER: usize=32768;
pub const MAX_BODY: usize=65536;
pub const IO_TIMEOUT: Duration=Duration::from_secs(8);
pub const CONNECT_TIMEOUT: Duration=Duration::from_secs(3);

pub struct Request {
    pub path: String,
    pub query: String,
    pub bearer: Option<String>,
}

fn local_authority(value: &str) -> bool {
    let (host, port)=match value.split_once(':') { Some((h,p))=>(h,Some(p)), None=>(value,None) };
    (host.eq_ignore_ascii_case("localhost") || host=="127.0.0.1")
        && port.map_or(true, |p| p.parse::<u16>().is_ok_and(|n| n!=0))
}

/// Parse one bounded GET. The connection closes after this request: no pipelining.
pub fn parse_request(bytes: &[u8]) -> Result<Request, &'static str> {
    if bytes.len()>MAX_HEADER { return Err("header_too_large"); }
    let text=std::str::from_utf8(bytes).map_err(|_| "header_encoding")?;
    if !text.ends_with("\r\n\r\n") { return Err("header_terminator"); }
    let mut lines=text[..text.len()-4].split("\r\n");
    let first=lines.next().ok_or("request_line")?;
    let words:Vec<_>=first.split(' ').collect();
    if words.len()!=3 || words[0]!="GET" || words[2]!="HTTP/1.1" { return Err("GET_HTTP_1_1_required"); }
    let target=words[1];
    if !target.starts_with('/') || target.starts_with("//") || target.bytes().any(|b| b<=32 || b>=127 || b==b'#') { return Err("request_target"); }
    let mut host=None; let mut bearer=None; let mut seen=Vec::new();
    for (count,line) in lines.enumerate() {
        if count>=32 { return Err("too_many_headers"); }
        let (name,value)=line.split_once(':').ok_or("header_shape")?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b==b'-') { return Err("header_name"); }
        let name=name.to_ascii_lowercase();
        if seen.contains(&name) { return Err("duplicate_header"); }
        seen.push(name.clone());
        let value=value.trim();
        if value.bytes().any(|b| !(32..127).contains(&b)) { return Err("header_value"); }
        match name.as_str() {
            "host"=> { if !local_authority(value) { return Err("remote_host_refused"); } host=Some(value); }
            "origin"=> { if !value.strip_prefix("http://").is_some_and(local_authority) { return Err("remote_origin_refused"); } }
            "authorization"=>bearer=Some(value.strip_prefix("Bearer ").ok_or("bearer_shape")?.to_owned()),
            "content-length" if value!="0"=>return Err("request_body_refused"),
            "transfer-encoding"=>return Err("request_body_refused"),
            _=>{},
        }
    }
    if host.is_none() { return Err("host_required"); }
    let (path,query)=target.split_once('?').unwrap_or((target,""));
    Ok(Request {path:path.to_owned(),query:query.to_owned(),bearer})
}

fn read_header(stream:&mut TcpStream) -> Result<(Vec<u8>,Vec<u8>), &'static str> {
    let mut bytes=Vec::with_capacity(1024);
    loop {
        if bytes.len()>=MAX_HEADER { return Err("header_too_large"); }
        let mut chunk=[0;1024];
        let limit=chunk.len().min(MAX_HEADER-bytes.len());
        let n=stream.read(&mut chunk[..limit]).map_err(|_| "socket_read_not_measured")?;
        if n==0 { return Err("header_incomplete"); }
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(i)=bytes.windows(4).position(|p|p==b"\r\n\r\n") {
            let remainder=bytes.split_off(i+4);
            return Ok((bytes,remainder));
        }
    }
}
pub fn request(stream:&mut TcpStream) -> Result<Request,&'static str> {
    stream.set_read_timeout(Some(IO_TIMEOUT)).map_err(|_| "socket_timeout_config")?;
    stream.set_write_timeout(Some(IO_TIMEOUT)).map_err(|_| "socket_timeout_config")?;
    let (header,extra)=read_header(stream)?;
    if !extra.is_empty() { return Err("request_body_refused"); }
    parse_request(&header)
}
pub fn send(stream:&mut TcpStream,status:u16,body:&str) -> std::io::Result<()> {
    let label=match status {200=>"OK",400=>"Bad Request",401=>"Unauthorized",404=>"Not Found",503=>"Service Unavailable",_=>"Bad Gateway"};
    write!(stream,"HTTP/1.1 {status} {label}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\r\n{body}",body.len())?;
    stream.flush()
}
pub fn fetch(addr:std::net::SocketAddr,key:&str,target:&str) -> Result<String,&'static str> {
    let mut stream=TcpStream::connect_timeout(&addr,CONNECT_TIMEOUT).map_err(|_| "peer_connect_not_measured")?;
    stream.set_read_timeout(Some(IO_TIMEOUT)).map_err(|_| "peer_timeout_config")?;
    stream.set_write_timeout(Some(IO_TIMEOUT)).map_err(|_| "peer_timeout_config")?;
    write!(stream,"GET {target} HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {key}\r\nConnection: close\r\n\r\n").map_err(|_| "peer_write_not_measured")?;
    let (head,mut bytes)=read_header(&mut stream)?;
    let head=std::str::from_utf8(&head).map_err(|_| "peer_header_encoding")?;
    let mut lines=head.trim_end_matches("\r\n").split("\r\n");
    if lines.next()!=Some("HTTP/1.1 200 OK") { return Err("peer_http_not_ok"); }
    let mut length=None;
    let mut content_type=false;
    for line in lines {
        let (name,value)=line.split_once(':').ok_or("peer_header_shape")?;
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() { return Err("peer_duplicate_length"); }
            length=Some(value.trim().parse::<usize>().map_err(|_| "peer_length")?);
        }
        if name.eq_ignore_ascii_case("transfer-encoding") { return Err("peer_transfer_encoding"); }
        if name.eq_ignore_ascii_case("content-type") { content_type=value.trim().starts_with("text/plain"); }
    }
    let length=length.ok_or("peer_length_required")?;
    if length>MAX_BODY || bytes.len()>length || !content_type { return Err("peer_body_shape"); }
    let present=bytes.len(); bytes.resize(length,0);
    stream.read_exact(&mut bytes[present..]).map_err(|_| "peer_body_not_measured")?;
    let body=String::from_utf8(bytes).map_err(|_| "peer_body_encoding")?;
    if body.is_empty() || !body.ends_with('\n') || body.lines().any(|s| !s.ends_with("|json=0") || s.contains(['\r','\0'])) { return Err("peer_HBP_required"); }
    Ok(body)
}
pub fn encode(text:&str)->String {
    let mut out=String::new();
    const HEX:&[u8;16]=b"0123456789ABCDEF";
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b,b'-'|b'_'|b'.'|b'~') { out.push(char::from(b)); }
        else { out.push('%');out.push(char::from(HEX[usize::from(b>>4)]));out.push(char::from(HEX[usize::from(b&15)])); }
    }
    out
}
pub fn single_query(query:&str,key:&str)->Result<String,&'static str> {
    let (name,value)=query.split_once('=').ok_or("query_shape")?;
    if name!=key || value.contains('&') { return Err("query_shape"); }
    let mut out=Vec::new(); let bytes=value.as_bytes(); let mut i=0;
    while i<bytes.len() {
        let b=match bytes[i] {
            b'%'=> {
                let p=bytes.get(i+1..i+3).ok_or("query_percent")?;
                let a=(p[0] as char).to_digit(16).ok_or("query_percent")?;
                let b=(p[1] as char).to_digit(16).ok_or("query_percent")?;
                i+=2;(a*16+b) as u8
            }
            b'+'=>b' ',
            b=>b,
        };
        if !(32..127).contains(&b) { return Err("query_ASCII_required"); }
        out.push(b); i+=1;
    }
    String::from_utf8(out).map_err(|_| "query_ASCII_required")
}
