

fn main() -> Result<(), Error> {
    let server_address = std::env::args().skip(1).next().expect("require nng url to listen on");

    let socket = nng::Socket::new(Protocol::Rep0)?;
    socket.listen(server_address)?;

}
