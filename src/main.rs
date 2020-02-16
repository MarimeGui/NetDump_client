use clap::Clap;
use ez_io::{MagicNumberCheck, ReadE, WriteE};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::Serialize;
use serde_json::to_writer_pretty;
use std::fmt::{Display, Formatter, Result as FMTResult};
use std::fs::File;
use std::io::{stdout, BufWriter, Read, Write};
use std::mem::transmute;
use std::net::TcpStream;

static PROTOCOL_VERSION: u32 = 1;
static MAGIC_NUMBER: &str = "NETDUMP";

static IO_SIZE: usize = 32768;

// ---------------------------- CLI Argument Parsing stuff ----------------------------

/// Client for netdump running on a Wii
#[derive(Clap)]
#[clap(author = "Marime Gui <lepro.guillaume@gmail.com>")]
struct Opts {
    /// Hostname of the Wii to connect to
    #[clap(short = "a", long = "address", value_name = "HOSTNAME")]
    host_address: String,
    /// Hostname of the Wii to connect to
    #[clap(
        short = "p",
        long = "port",
        value_name = "PORT",
        default_value = "9875"
    )]
    port: u16,
    /// Choose what to get from the disc
    #[clap(subcommand)]
    process: Process,
}

#[derive(Clap)]
enum Process {
    /// Dumps the Game, BCA and Info to three separate files
    #[clap(name = "full")]
    Full(FullDump),
    /// Dumps game ISO only
    #[clap(name = "game")]
    Game(GameDump),
    /// Dumps game BCA only
    #[clap(name = "bca")]
    BCA(BCADump),
    /// Returns Disc type (GC, Wii Single-sided or Wii Double-sided), Game ID and Game Name
    #[clap(name = "info")]
    Info(InfoRead),
    /// Eject the Disc from the Drive
    #[clap(name = "eject")]
    EjectDisc,
    /// Exits the program on the Wii
    #[clap(name = "exit")]
    ExitProgram,
    /// Shutdown the Wii
    #[clap(name = "shutdown")]
    Shutdown,
}

#[derive(Clap)]
struct FullDump {
    /// Where the files will be written to
    #[clap(
        short = "o",
        long = "output",
        value_name = "DIRECTORY",
        default_value = "."
    )]
    location: String,
}

#[derive(Clap)]
struct GameDump {
    /// Where to write game dump
    #[clap(
        short = "o",
        long = "output",
        value_name = "FILE",
        default_value = "./game.iso"
    )]
    filepath: String,
    /// Output to stdout rather than to a file. If this is set, 'output' option will be ignored
    #[clap(short = "s", long = "stdout")]
    stdout: bool,
}

#[derive(Clap)]
struct BCADump {
    /// Where to write BCA dump
    #[clap(
        short = "o",
        long = "output",
        value_name = "FILE",
        default_value = "./game.bca"
    )]
    filepath: String,
    /// Output to stdout rather than to a file. If this is set, 'output' option will be ignored
    #[clap(short = "s", long = "stdout")]
    stdout: bool,
}

#[derive(Clap)]
struct InfoRead {
    /// Write info dump as JSON to a file
    #[clap(short = "o", long = "output", value_name = "FILE")]
    filepath: Option<String>,
}

// ---------------------------- Network Protocol Stuff ----------------------------

#[derive(Copy, Clone)]
#[repr(u32)]
enum Commands {
    /// Ask to disconnect nicely
    Disconnect = 0xFFFF_FFFF,
    /// Exit program on the Wii
    ExitProgram = 0xFFFF_FFFE,
    /// Shutdown console, acts like we're disconnecting
    Shutdown = 0xFFFF_FFFD,
    /// Ejects the Disc
    EjectDisc = 1,
    /// Get info about the disc
    GetDiscInfo = 2,
    /// Dumps BCA
    DumpBCA = 3,
    /// Dumps main Game
    DumpGame = 4,
}

#[derive(Copy, Clone, FromPrimitive)]
#[repr(u32)]
enum CommandAnswers {
    ProtocolError = 0xFFFF_FFFF,
    NoDisc = 0xFFFF_FFFE,
    CouldNotEject = 0xFFFF_FFFD,
    UnknownDiscType = 0xFFFF_FFFC,
    OK = 0,
    DiscInfo = 1,
    BCA = 2,
    Game = 3,
}

#[derive(Copy, Clone, FromPrimitive, Serialize)]
#[repr(u8)]
pub enum DiscTypes {
    GC = 0,
    WiiSingleSided = 1,
    WiiDoubleSided = 2,
}

impl Display for DiscTypes {
    fn fmt(&self, f: &mut Formatter<'_>) -> FMTResult {
        match &self {
            DiscTypes::GC => write!(f, "GameCube"),
            DiscTypes::WiiSingleSided => write!(f, "Wii Single-Sided"),
            DiscTypes::WiiDoubleSided => write!(f, "Wii Double-Sided"),
        }
    }
}

#[derive(Serialize)]
pub struct DiscInfo {
    pub disc_type: DiscTypes,
    pub game_name: String,
    pub internal_name: String,
}

macro_rules! send_command {
    ($stream:expr, $command:ident) => {
        let mut packet = Vec::with_capacity(15);
        packet.write_all(&MAGIC_NUMBER.as_bytes()).unwrap(); // Magic Number
        packet.write_be_to_u32(PROTOCOL_VERSION).unwrap(); // Protocol Version
        packet.write_be_to_u32(Commands::$command as u32).unwrap(); // Command, 'as' is meh
        $stream.write_all(&packet).unwrap();
    };
}

macro_rules! send_disconnect {
    ($stream:expr) => {
        send_command!($stream, Disconnect);

        check_magic_number_protocol_version!($stream);

        match CommandAnswers::from_u32($stream.read_be_to_u32().unwrap()) {
            Some(CommandAnswers::OK) => {}
            _ => eprintln!("Weird response from Wii, disconnecting anyways"),
        }
    };
}

macro_rules! check_magic_number_protocol_version {
    ($stream:expr) => {
        $stream
            .check_magic_number(&MAGIC_NUMBER.as_bytes())
            .unwrap(); // Check Magic Number
        $stream
            .check_magic_number(unsafe { &transmute::<u32, [u8; 4]>(PROTOCOL_VERSION.to_be()) })
            .unwrap(); // Check Protocol Version, Meh transmute
    };
}

// ---------------------------- Main Code ----------------------------

fn main() {
    let opts: Opts = Opts::parse();

    let mut stream = TcpStream::connect(format!("{}:{}", opts.host_address, opts.port))
        .expect("Failed to connect to the Wii");

    match opts.process {
        Process::ExitProgram => {
            send_command!(stream, ExitProgram);

            check_magic_number_protocol_version!(stream);

            match CommandAnswers::from_u32(stream.read_be_to_u32().unwrap()) {
                Some(CommandAnswers::OK) => {}
                _ => eprintln!("Weird response from Wii"),
            }
        }
        Process::Shutdown => {
            send_command!(stream, Shutdown);

            check_magic_number_protocol_version!(stream);

            match CommandAnswers::from_u32(stream.read_be_to_u32().unwrap()) {
                Some(CommandAnswers::OK) => {}
                _ => eprintln!("Weird response from Wii"),
            }
        }
        Process::Full(o) => {
            unimplemented!();
        }
        Process::Info(i) => {
            send_command!(stream, GetDiscInfo);

            check_magic_number_protocol_version!(stream);

            match CommandAnswers::from_u32(stream.read_be_to_u32().unwrap()) {
                Some(CommandAnswers::DiscInfo) => {
                    let disc_type_u8 = stream.read_to_u8().unwrap();

                    let mut game_name_buf = vec![0u8; 32];
                    stream.read_exact(&mut game_name_buf).unwrap();
                    let game_name = String::from_utf8(game_name_buf)
                        .unwrap()
                        .trim_end_matches(char::from(0))
                        .to_string();

                    let mut internal_name_buf = vec![0u8; 512];
                    stream.read_exact(&mut internal_name_buf).unwrap();
                    let internal_name = String::from_utf8(internal_name_buf)
                        .unwrap()
                        .trim_end_matches(char::from(0))
                        .to_string();

                    let disc = DiscInfo {
                        disc_type: DiscTypes::from_u8(disc_type_u8).unwrap(),
                        game_name,
                        internal_name,
                    };

                    match i.filepath {
                        None => {
                            println!("Disc Type: {}", disc.disc_type);
                            println!("Game Name: {}", disc.game_name);
                            println!("Internal Name: {}", disc.internal_name);
                        }
                        Some(p) => {
                            let file = &mut File::create(p).unwrap();
                            to_writer_pretty(file, &disc).unwrap();
                        }
                    }
                }
                Some(CommandAnswers::ProtocolError) => {
                    eprintln!("Unknown Protocol-related error, can't proceed");
                }
                Some(CommandAnswers::NoDisc) => {
                    eprintln!("No Disc in Drive, can't proceed");
                }
                Some(CommandAnswers::UnknownDiscType) => {
                    eprintln!("Unknown Disc Type, can't proceed");
                }
                _ => {
                    eprintln!("Weird response from Wii, disconnecting");
                }
            }

            send_disconnect!(stream);
        }
        Process::BCA(bca) => {
            send_command!(stream, DumpBCA);

            check_magic_number_protocol_version!(stream);

            match CommandAnswers::from_u32(stream.read_be_to_u32().unwrap()) {
                Some(CommandAnswers::BCA) => {
                    let mut writer: Box<dyn Write> = if bca.stdout {
                        Box::new(stdout())
                    } else {
                        Box::new(BufWriter::new(
                            File::create(bca.filepath).expect("Failed to open file"),
                        ))
                    };

                    let mut data = vec![0u8; 64]; // Lossy
                    stream.read_exact(&mut data).unwrap();
                    writer.write_all(&data).unwrap();
                }
                Some(CommandAnswers::ProtocolError) => {
                    eprintln!("Unknown Protocol-related error, can't proceed");
                }
                Some(CommandAnswers::NoDisc) => {
                    eprintln!("No Disc in Drive, can't proceed");
                }
                Some(CommandAnswers::UnknownDiscType) => {
                    eprintln!("Unknown Disc Type, can't dump");
                }
                _ => {
                    eprintln!("Weird response from Wii, disconnecting");
                }
            }

            send_disconnect!(stream);
        }
        Process::Game(g) => {
            send_command!(stream, DumpGame);

            check_magic_number_protocol_version!(stream);

            match CommandAnswers::from_u32(stream.read_be_to_u32().unwrap()) {
                Some(CommandAnswers::Game) => {
                    let mut writer: Box<dyn Write> = if g.stdout {
                        Box::new(stdout())
                    } else {
                        Box::new(BufWriter::new(
                            File::create(g.filepath).expect("Failed to open file"),
                        ))
                    };

                    let data_length = stream.read_be_to_u64().unwrap();
                    let mut data_received = 0u64;
                    let mut data = vec![0u8; IO_SIZE];
                    while data_received < data_length {
                        if (data_length - data_received) < (IO_SIZE as u64) {
                            // Last data parts might not be big enough to fit buffer
                            let mut last_data = vec![0u8; (data_length - data_received) as usize]; // Lossy
                            stream.read_exact(&mut last_data).unwrap();
                            writer.write_all(&last_data).unwrap();
                            data_received += data_length - data_received;
                        } else {
                            stream.read_exact(&mut data).unwrap();
                            writer.write_all(&data).unwrap();
                            data_received += IO_SIZE as u64; // Lossy
                        }
                    }
                }
                Some(CommandAnswers::ProtocolError) => {
                    eprintln!("Unknown Protocol-related error, can't proceed");
                }
                Some(CommandAnswers::NoDisc) => {
                    eprintln!("No Disc in Drive, can't proceed");
                }
                Some(CommandAnswers::UnknownDiscType) => {
                    eprintln!("Unknown Disc Type, can't dump");
                }
                _ => {
                    eprintln!("Weird response from Wii, disconnecting");
                }
            }

            send_disconnect!(stream);
        }
        Process::EjectDisc => {
            send_command!(stream, EjectDisc);

            check_magic_number_protocol_version!(stream);

            match CommandAnswers::from_u32(stream.read_be_to_u32().unwrap()) {
                Some(CommandAnswers::OK) => {}
                Some(CommandAnswers::NoDisc) => println!("No Disc in drive"),
                Some(CommandAnswers::CouldNotEject) => println!("Couldn't Eject Disc"),
                Some(CommandAnswers::ProtocolError) => {
                    eprintln!("Unknown Protocol-related error, can't proceed")
                }
                _ => eprintln!("Weird response from Wii"),
            }

            send_disconnect!(stream);
        }
    }
}
