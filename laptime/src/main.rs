use actix_web::{web, App, HttpResponse, HttpServer, Responder, FromRequest, middleware::Logger};
use actix_web::http::StatusCode;
use actix_web::body::to_bytes;
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, Arc};
use chrono::Utc;
use actix_files as fs;
use serialport::SerialPort;
use std::fs::File;
use std::io::{self, BufReader, BufRead, Write, Read};
use clap::Parser;
use actix_cors::Cors;
use log::{info, error, log, debug}; // Lisätty log-makrot
use tokio::{test, time::sleep};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "sim")]
    mode: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Clock {
    #[serde(skip_deserializing)]
    id: u32, // Uniikki tunniste
    hour: u32,
    minute: u32,
    second: f32,
    sector1: f32,
    sector2: f32,
    sector3: f32,
    timestamp: String,
    lap_count: u32,
    device_type: DeviceType,
    rfid_read_time: String,
    total_time: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
enum DeviceType {
    Anchor,
    Tag,
}

struct AppState {
    clocks: Arc<Mutex<Vec<Clock>>>,
}

async fn get_lap_times(data: web::Data<AppState>) -> impl Responder {
    let clocks = data.clocks.lock().unwrap();
    HttpResponse::Ok().json(&(*clocks) )
}

async fn get_uwb_measurements() -> impl Responder {
    // Simulaatio UWB-mittauksille
    let measurements = uwb::read_measurements();
    HttpResponse::Ok().json(&measurements)
}

async fn get_position() -> impl Responder {
    // Simulaatio paikannukselle
    let position = positioning::calculate_position();
    HttpResponse::Ok().json(&position)
}

/// Lisää uuden kierrosajan tietokantaan.
///
/// Tämä funktio lisää uuden kierrosajan tietokantaan ja tallentaa sen
/// tiedostoon. Jos kierrosajan `total_time`-arvo on negatiivinen, palautetaan
/// HTTP-vastaus 400 Bad Request.
///
/// # Example
///
/// `curl -X POST -H "Content-Type: application/json" -d '{"hour": 12, "minute": 34, "second": 56.78, "sector1": 10.5, "sector2": 20.3, "sector3": 15.2, "timestamp": "2025-02-12T15:31:31.150502300Z", "lap_count": 1, "device_type": "Anchor", "rfid_read_time": "2025-02-12T15:31:31.151157300Z", "total_time": 50.5 }' http://localhost:8080/api/lap_times`
async fn add_lap_time(
    new_lap: web::Json<Clock>,
    data: web::Data<AppState>,
) -> HttpResponse {
    debug!("add_lap_time: Funktio käynnistyi!");
    let lap_data = new_lap.into_inner();
    debug!("Pyynnön runko: {:?}", lap_data);

    // Tarkistetaan, että jokin arvo on validi
    if lap_data.total_time < 0.0 {
        error!("Kierrosaika on negatiivinen! Lopetetaan.");
        return HttpResponse::BadRequest().body("Negatiivinen kierrosaika ei sallittu");
    }

    // Lukitse data
    let mut clocks_mut = data.clocks.lock().unwrap();

    debug!("Sain lukon, tallennetaan data muistiin.");
    clocks_mut.push(lap_data.clone());
    drop(clocks_mut); // vapauta lukko mahdollisimman pian

    // Yritä tallentaa tiedostoon
    match save_lap_times(&data.clocks) {
        Ok(_) => {
            info!("Kierrosaika tallennettu tiedostoon onnistuneesti.");
            HttpResponse::Created().json(&lap_data)
        }
        Err(e) => {
            error!("Virhe tallennettaessa kierrosaikoja: {}", e);
            HttpResponse::InternalServerError().body("Tiedostoon tallennus epäonnistui")
        }
    }
}



/// PUT /api/lap_times/{id}
/// Päivittää kierrosaika-alkion indeksillä `id`.
async fn update_lap_time(
    id: web::Path<usize>,
    updated_lap: web::Json<Clock>,
    data: web::Data<AppState>,
) -> HttpResponse {
    let lap_id = id.into_inner();
    info!("update_lap_time funktio käynnistetty, id = {}", lap_id);

    let updated_lap_data = updated_lap.into_inner();
    info!("Vastaanotettu päivitysdata (request body): {:?}", updated_lap_data);

    // Validointiesimerkki: ei sallita negatiivista kokonaisaikaa
    if updated_lap_data.total_time < 0.0 {
        error!("Validointivirhe päivityksessä: Kokonaisaika ei voi olla negatiivinen!");
        return HttpResponse::BadRequest().body("Virheellinen data: Kokonaisaika negatiivinen");
    }

    // Lukitaan data vain kerran
    let mut clocks_mut = data.clocks.lock().unwrap();

    // Tarkistetaan, onko indeksin arvo validi
    if lap_id >= clocks_mut.len() {
        error!("Kierrosaikaa indeksillä {} ei löydy päivitettäväksi.", lap_id);
        return HttpResponse::NotFound().body(format!("No lap time found with id: {}", lap_id));
    }

    // Päivitetään haluttu kierrosaika
    clocks_mut[lap_id] = updated_lap_data.clone();
    drop(clocks_mut); // Vapautetaan lukko mahdollisimman pian

    info!("Kierrosaika indeksillä {} päivitetty onnistuneesti muistiin.", lap_id);

    // Tallennetaan muutokset tiedostoon
    match save_lap_times(&data.clocks) {
        Ok(_) => {
            info!("Päivitetyt kierrosajat tallennettu tiedostoon onnistuneesti.");
            let response = HttpResponse::Ok().json(&updated_lap_data);
            info!("Palautetaan HTTP-vastaus: status={}, body={:?}",
                  response.status(), response.body());
            response
        }
        Err(e) => {
            error!("Virhe tallennettaessa päivitettyjä kierrosaikoja tiedostoon: {}", e);
            HttpResponse::InternalServerError().body("Failed to save updated lap times to file.")
        }
    }
}

/// DELETE /api/lap_times/{id}
/// Poistaa kierrosaika-alkion indeksillä `id`.
async fn delete_lap_time(
    id: web::Path<usize>,
    data: web::Data<AppState>,
) -> HttpResponse {
    let lap_id = id.into_inner();
    info!("delete_lap_time funktio käynnistetty, id = {}", lap_id);

    let mut clocks_mut = data.clocks.lock().unwrap();
    if lap_id >= clocks_mut.len() {
        error!("Kierrosaikaa indeksillä {} ei löydy poistettavaksi.", lap_id);
        return HttpResponse::NotFound().body(format!("No lap time found with id: {}", lap_id));
    }

    // Poistetaan haluttu kierrosaika
    clocks_mut.remove(lap_id);
    drop(clocks_mut); // Vapautetaan lukko heti poiston jälkeen

    info!("Kierrosaika indeksillä {} poistettu onnistuneesti muistista.", lap_id);

    // Tallennetaan muutokset tiedostoon
    match save_lap_times(&data.clocks) {
        Ok(_) => {
            info!("Poiston jälkeinen tallennus tiedostoon onnistui.");
            HttpResponse::NoContent().finish()
        }
        Err(e) => {
            error!("Virhe tallennettaessa kierrosaikoja tiedostoon poiston jälkeen: {}", e);
            HttpResponse::InternalServerError().body("Failed to save lap times to file after delete.")
        }
    }
}

fn read_lap_times_from_file() -> Result<Vec<Clock>, io::Error> {
    let file = File::open("laps.json")?;
    let reader = BufReader::new(file);
    let mut laps = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if let Ok(lap) = serde_json::from_str::<Clock>(&line) {
            laps.push(lap);
        }
    }
    Ok(laps)
}

fn save_lap_times(clocks: &Arc<Mutex<Vec<Clock>>>) -> Result<(), Box<dyn std::error::Error>> {
    let clocks_locked = clocks.lock().unwrap();
    let file = File::create("laps.json")?;
    let mut writer = io::BufWriter::new(file);
    for lap in clocks_locked.iter() {
        serde_json::to_writer(&mut writer, &lap)?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

mod uwb {
    use serde_json::json;

    pub fn read_measurements() -> serde_json::Value {
        // Simuloitu data UWB-mittauksille
        json!([
            {"device_id": "UWB_1", "anchor_id": "ANCHOR_A", "distance": 10.5},
            {"device_id": "UWB_1", "anchor_id": "ANCHOR_B", "distance": 20.3},
            {"device_id": "UWB_2", "anchor_id": "ANCHOR_C", "distance": 15.2}
        ])
    }
}

mod positioning {
    use serde_json::json;

    pub fn calculate_position() -> serde_json::Value {
        // Simuloitu data paikannukselle
        json!({
            "device_id": "UWB_1",
            "position": {"x": 1.23, "y": 4.56, "z": 0.78}
        })
    }
}



#[actix_web::main]
async fn main() -> io::Result<()> {


    env_logger::init(); // Alusta loggeri
    info!("Loggeri alustettu MAIN-funktiossa!"); // LISÄTTY TESTILOKI

    let laps = match read_lap_times_from_file() {
        Ok(laps) => laps,
        Err(_) => {
            eprintln!("Warning: laps.json file not found or corrupted. Starting with empty laps data.");
            Vec::new()
        }
    };



    let app_state = web::Data::new(AppState {
        clocks: Arc::new(Mutex::new(laps)),
    });

    HttpServer::new(move || {
        // CORS middleware lisätty tähän
        let cors = Cors::permissive(); // Kehitykseen, älä tuotantoon!

//        let args = Args::parse();
//        println!("Running in mode: {}", args.mode);

        App::new()
            .wrap(cors) // Lisää CORS middleware
            .wrap(Logger::default())
            .app_data(app_state.clone())
            .service(
                web::scope("/api")
                    .route("/lap_times", web::get().to(get_lap_times))
                    .route("/add_lap", web::post().to(add_lap_time))
                    .route("/lap_times/{id}", web::put().to(update_lap_time))
                    .route("/lap_times/{id}", web::delete().to(delete_lap_time))
                    .route("/uwb", web::get().to(get_uwb_measurements))
                    .route("/position", web::get().to(get_position))
            )
            .service(fs::Files::new("/", "./html").index_file("index.html"))
    })
        .bind("127.0.0.1:8080")?
        .run()
        .await
}


#[test]
async fn test_add_lap_time_happy_path() -> Result<(), actix_web::Error> {
    let clock = Clock {
        id: 1,
        hour: 12,
        minute: 34,
        second: 56.78,
        sector1: 10.5,
        sector2: 20.3,
        sector3: 15.2,
        timestamp: "2025-02-12T15:31:31.150502300Z".into(),
        lap_count: 1,
        device_type: DeviceType::Anchor,
        rfid_read_time: "2025-02-12T15:31:31.151157300Z".into(),
        total_time: 50.5,
    };

    let app_state = web::Data::new(AppState {
        clocks: Arc::new(Mutex::new(Vec::new())),
    });

    // Clone clock for the comparison later
    let clock_clone = clock.clone();

    // Store response status before consuming the response
    let response = add_lap_time(web::Json(clock), app_state).await;
    let status = response.status();

    // Convert body to bytes and parse
    let body = to_bytes(response.into_body()).await?;
    let json_body: Clock = serde_json::from_slice(&body)?;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json_body, clock_clone);

    Ok(())
}