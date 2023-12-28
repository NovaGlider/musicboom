use std::error::Error;

use biquad::{Biquad, Coefficients, DirectForm2Transposed, ToHertz, Q_BUTTERWORTH_F32};
use buttplug::{
    client::{ButtplugClient, ScalarValueCommand},
    core::connector::new_json_ws_client_connector,
};
use clap::Parser;
use jack::{
    AudioIn, Client, ClientOptions, ClosureProcessHandler, Control, NotificationHandler, PortFlags,
};

struct Notifications;

impl NotificationHandler for Notifications {}

enum Message {
    Data { data: Vec<f32>, sample_rate: usize },
    Quit,
}

fn float_to_bar(x: f64, n: usize) -> String {
    let mut res = String::new();
    let nf = n as f64;
    for i in 0..n {
        if x >= (i as f64) / (nf - 1.) {
            res += "█";
        } else {
            res += "░";
        }
    }
    res
}

#[derive(Parser)]
struct Opts {
    #[arg(long, short, help = "show debug output")]
    debug: bool,
    #[arg(
        long,
        short,
        help = "frequency in Hz for lowp ass",
        default_value = "200.0"
    )]
    low: f32,
    #[arg(
        long,
        short = 'f',
        help = "frequency in Hz for high pass",
        default_value = "1000.0"
    )]
    high: f32,
    #[arg(
        long,
        short,
        help = "linear amplification for vibration",
        default_value = "1.1"
    )]
    amp: f64,
    #[arg(
        long,
        short,
        help = "URI to Intiface",
        default_value = "ws://localhost:12345/ws"
    )]
    uri: String,
    #[arg(
        help = "connect to all Jack ports with this in their name",
        default_value = "output"
    )]
    filter: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Opts::parse();

    let bp = ButtplugClient::new("MusicBoom");
    let connector = new_json_ws_client_connector(&args.uri);
    bp.connect(connector).await?;
    bp.start_scanning().await?;
    bp.stop_scanning().await?;
    let bp_device = bp.devices()[0].clone();

    let (client, _client_status) = Client::new("MusicBoom", ClientOptions::NO_START_SERVER)?;

    let port = client.register_port("MusicBoom", AudioIn::default())?;

    let target_ports = client.ports(
        Some(&format!(".*{}.*", args.filter)),
        None,
        PortFlags::IS_OUTPUT,
    );
    if target_ports.is_empty() {
        println!("No ports found, available:");
        for port in client.ports(None, None, PortFlags::IS_OUTPUT) {
            println!(" - {port}");
        }
        return Ok(());
    }
    for target_port_name in target_ports {
        let target_port = client.port_by_name(&target_port_name).unwrap();
        match client.connect_ports(&target_port, &port) {
            Ok(_) => println!("Connected to port {target_port_name}"),
            Err(_) => println!("Failed to connect to port {target_port_name}"),
        }
    }

    let (tx, rx) = std::sync::mpsc::channel();

    let handle = tokio::task::spawn(async move {
        let mut max_value = 1e-10;
        let mut total_max_value = [1e-10, 1e-10];
        while let Ok(Message::Data { data, sample_rate }) = rx.recv() {
            let mut values: Vec<f64> = [
                Coefficients::<f32>::from_params(
                    biquad::Type::LowPass,
                    (sample_rate as u32).hz(),
                    args.low.hz(),
                    Q_BUTTERWORTH_F32,
                )
                .unwrap(),
                Coefficients::<f32>::from_params(
                    biquad::Type::HighPass,
                    (sample_rate as u32).hz(),
                    2000.hz(),
                    Q_BUTTERWORTH_F32,
                )
                .unwrap(),
            ]
            .into_iter()
            .map(DirectForm2Transposed::<f32>::new)
            .enumerate()
            .map(|(i, mut b)| {
                let mut res = Vec::with_capacity(data.len());
                for &d in &data {
                    res.push(b.run(d) as f64);
                }
                // let num = res.len() as f64;
                // let mut x = res.iter().map(|x| x.abs()).sum::<f64>() / num;
                let mut x = res
                    .iter()
                    .map(|x| x.abs())
                    // .max_by(|a, b| a.total_cmp(b))
                    .min_by(|a, b| a.total_cmp(b))
                    .unwrap_or_default();
                if x > total_max_value[i] {
                    total_max_value[i] = x;
                }
                x /= total_max_value[i];
                if x > max_value {
                    max_value = x;
                }
                x
            })
            .collect();
            for (i, x) in values.iter_mut().enumerate() {
                *x /= max_value;
                *x *= args.amp;
                if *x > 1. {
                    *x = 1.;
                }
                if *x < 0. {
                    *x = 0.;
                }
                total_max_value[i] *= 0.9999;
            }
            max_value *= 0.999;

            if args.debug {
                print!("{max_value:.4?} {total_max_value:.4?}");
                for &x in &values {
                    print!(" {}", float_to_bar(x, 20));
                }
                println!();
            }
            bp_device
                .vibrate(&ScalarValueCommand::ScalarValueVec(values))
                .await
                .unwrap();
        }
    });

    let async_client = {
        let tx = tx.clone();
        client.activate_async(
            Notifications,
            ClosureProcessHandler::new(move |client, scope| {
                let sample_rate = client.sample_rate();
                let data = port.as_slice(scope).to_vec();
                match tx.send(Message::Data { data, sample_rate }) {
                    Ok(_) => Control::Continue,
                    Err(e) => {
                        eprintln!("{e}");
                        Control::Quit
                    }
                }
            }),
        )?
    };

    std::io::stdin().read_line(&mut String::new())?;

    async_client.deactivate()?;
    tx.send(Message::Quit)?;
    handle.await?;

    Ok(())
}
