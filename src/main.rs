use actix::{Actor, StreamHandler, AsyncContext};
use actix_web::{get, App, HttpServer, HttpRequest, Responder, web};
use actix_web_actors::ws;
use actix_files::Files; // Lisää tämä rivi
use rand::Rng;
use std::time::{Duration, Instant};
use actix_rt::time::interval;
use std::fs::OpenOptions;
use std::io::Write;
use tokio::signal;

// Tiedostoon tallentamisen apufunktio
fn log_temperature(temp: f64) {
    let file_path = "temperature_log.csv";
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(file_path)
        .expect("Unable to open file");

    if let Err(e) = writeln!(file, "{},{}", Instant::now().elapsed().as_secs(), temp) {
        eprintln!("Error writing to file: {}", e);
    }
}

// WebSocket-asiakas
struct TemperatureWebSocket {
    last_updated: Instant,
}

impl Actor for TemperatureWebSocket {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let mut rng = rand::rng();
        let mut ticker = interval(Duration::from_secs(1));

        // Lähetetään lämpötiloja joka sekunti
        ctx.run_interval(Duration::from_secs(1), move |act, ctx| {
            let temp1 = rng.random_range(-10.0..35.0); // Simuloitu lämpötila 1
            let temp2 = rng.random_range(-10.0..35.0); // Simuloitu lämpötila 2

            // Lähetetään molemmat lämpötilat asiakkaalle erotettuna putkimerkillä
            ctx.text(format!("{}|{}", temp1, temp2));
        });
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for TemperatureWebSocket {
    fn handle(&mut self, _: Result<ws::Message, ws::ProtocolError>, _: &mut Self::Context) {}
}

#[get("/")]
async fn index() -> impl Responder {
    // Staattinen index.html-tiedosto, joka on palvelimen "static" kansiossa
    actix_web::HttpResponse::Ok().body(include_str!("../static/index.html"))
}

#[get("/ws")]
async fn websocket(req: HttpRequest, stream: web::Payload) -> impl Responder {
    ws::start(TemperatureWebSocket { last_updated: Instant::now() }, &req, stream)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Palvelin käynnissä...");

    // Avaa ja kirjoita aloitusrivi CSV-tiedostoon
    log_temperature(0.0);

    // Käynnistetään Actix Web -palvelin
    let server = HttpServer::new(|| {
        App::new()
            .service(index) // Tarjoaa index.html:n
            .service(websocket) // WebSocket-palvelu
            .service(Files::new("/static", "./static").show_files_listing()) // Tarjoaa staattiset tiedostot
    })
        .bind("0.0.0.0:8080")?
        .run();

    // Odotetaan Ctrl+C-signaalia, että ohjelma voi lopettaa
    tokio::select! {
        _ = server => {},
        _ = signal::ctrl_c() => {
            println!("Ohjelma suljetaan...");
        },
    }

    Ok(())
}