//! The command line, read: what `helio` was asked to do ([`parse`]), and
//! the stream a `stream` or `watch` asks for ([`request`]); the help that
//! lists all of it is `help`'s. Nothing here does anything, so all of it is
//! tested on every system.
//!
//! A word out of place is an error that says which, not a word skipped: a
//! stream asked for with a flag misspelt would otherwise start as something
//! else.

use helio_client::session::{NativeMode, StreamRequest};
use helio_proto::control::{app, codec};
use helio_proto::permission::Permissions;

/// What `helio` was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `--help`: every command; `host --help` (`host`): the host's.
    Help {
        host: bool,
    },
    Identity,
    Discover,
    Pair {
        address: String,
    },
    PairAgent {
        address: String,
    },
    Hosts,
    AddHost {
        name: String,
        address: String,
        x25519: String,
        mlkem: String,
    },
    RemoveHost {
        name: String,
    },
    Wake {
        name: String,
    },
    /// `stream NAME FLAGS`, or `watch NAME FLAGS` (`watch`). The flags are
    /// read again by [`request`] once this display's mode is known: they
    /// are checked here, so a mistake is told before anything connects.
    Stream {
        name: String,
        flags: Vec<String>,
        watch: bool,
    },
    Host(HostCommand),
}

/// `helio host COMMAND`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostCommand {
    Enable,
    Disable,
    Status {
        json: bool,
    },
    Identity,
    Clients,
    /// Show what a client may do (`set: None`), or set it.
    Permissions {
        x25519: String,
        set: Option<Permissions>,
    },
    AddClient {
        name: String,
        x25519: String,
        mlkem: String,
        agent: bool,
        permissions: Option<Permissions>,
    },
    RemoveClient {
        x25519: String,
    },
    Restart,
    /// The PIN a device asking to pair shows, given to the running host.
    Pair {
        pin: String,
        device: Option<String>,
        permissions: Option<Permissions>,
    },
}

/// A command line that asks for nothing `helio` does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage {
    /// What is wrong with it; none for a bare `helio` (or `helio host`),
    /// which is answered with the help.
    pub why: Option<String>,
    /// About `helio host`: its help goes with it.
    pub host: bool,
}

impl Usage {
    fn wrong(why: impl Into<String>, host: bool) -> Usage {
        Usage {
            why: Some(why.into()),
            host,
        }
    }
}

fn is_help(word: &str) -> bool {
    matches!(word, "-h" | "--help" | "help")
}

/// `rest`, the words after `command`, as exactly the `names` it takes.
fn exactly<const N: usize>(
    command: &str,
    rest: &[String],
    names: [&str; N],
    host: bool,
) -> Result<[String; N], Usage> {
    let full = || {
        let prefix = if host { "host " } else { "" };
        format!("{prefix}{command} {}", names.join(" "))
            .trim_end()
            .to_string()
    };
    if rest.len() < N {
        return Err(Usage::wrong(
            format!("{command} needs {}: helio {}", names[rest.len()], full()),
            host,
        ));
    }
    if let Some(extra) = rest.get(N) {
        return Err(Usage::wrong(
            format!("unexpected {extra:?} after helio {}", full()),
            host,
        ));
    }
    Ok(std::array::from_fn(|i| rest[i].clone()))
}

/// What the command line `args` (without the program's name) asks for.
pub fn parse(args: &[String]) -> Result<Command, Usage> {
    let Some((command, rest)) = args.split_first() else {
        return Err(Usage {
            why: None,
            host: false,
        });
    };
    let command = command.as_str();
    // `--help` anywhere: the help for what comes before it.
    if is_help(command) || rest.iter().any(|w| w == "-h" || w == "--help") {
        let host = command == "host" || rest.first().is_some_and(|w| w == "host");
        return Ok(Command::Help { host });
    }
    Ok(match command {
        "identity" => {
            exactly(command, rest, [], false)?;
            Command::Identity
        }
        "discover" => {
            exactly(command, rest, [], false)?;
            Command::Discover
        }
        "hosts" => {
            exactly(command, rest, [], false)?;
            Command::Hosts
        }
        "pair" => {
            let [address] = exactly(command, rest, ["HOST[:PORT]"], false)?;
            Command::Pair { address }
        }
        "pair-agent" => {
            let [address] = exactly(command, rest, ["HOST[:PORT]"], false)?;
            Command::PairAgent { address }
        }
        "add-host" => {
            let [name, address, x25519, mlkem] =
                exactly(command, rest, ["NAME", "ADDR", "X25519", "MLKEM"], false)?;
            Command::AddHost {
                name,
                address,
                x25519,
                mlkem,
            }
        }
        "remove-host" => {
            let [name] = exactly(command, rest, ["NAME"], false)?;
            Command::RemoveHost { name }
        }
        "wake" => {
            let [name] = exactly(command, rest, ["NAME"], false)?;
            Command::Wake { name }
        }
        "stream" | "watch" => {
            let watch = command == "watch";
            let Some((name, flags)) = rest.split_first().filter(|(n, _)| !n.starts_with('-'))
            else {
                return Err(Usage::wrong(
                    format!("{command} needs the NAME of a paired host first: helio {command} NAME [FLAGS]"),
                    false,
                ));
            };
            request(flags, &NativeMode::default(), watch).map_err(|e| Usage::wrong(e, false))?;
            Command::Stream {
                name: name.clone(),
                flags: flags.to_vec(),
                watch,
            }
        }
        "host" if rest.first().is_some_and(|w| w == "help") => Command::Help { host: true },
        "host" => Command::Host(parse_host(rest)?),
        other => {
            return Err(Usage::wrong(format!("unknown command {other:?}"), false));
        }
    })
}

/// `helio host` and the words after it.
fn parse_host(args: &[String]) -> Result<HostCommand, Usage> {
    let Some((command, rest)) = args.split_first() else {
        return Err(Usage {
            why: None,
            host: true,
        });
    };
    let command = command.as_str();
    let permissions = |list: &str| Permissions::parse(list).map_err(|e| Usage::wrong(e, true));
    let unexpected = |word: &String, after: &str| {
        Usage::wrong(
            format!("unexpected {word:?} after helio host {after}"),
            true,
        )
    };
    Ok(match command {
        "enable" => {
            exactly(command, rest, [], true)?;
            HostCommand::Enable
        }
        "disable" => {
            exactly(command, rest, [], true)?;
            HostCommand::Disable
        }
        "restart" => {
            exactly(command, rest, [], true)?;
            HostCommand::Restart
        }
        "status" => match rest {
            [] => HostCommand::Status { json: false },
            [json] if json == "--json" => HostCommand::Status { json: true },
            [json, extra, ..] if json == "--json" => {
                return Err(unexpected(extra, "status --json"))
            }
            [other, ..] => return Err(unexpected(other, "status (--json)")),
        },
        "identity" => {
            exactly(command, rest, [], true)?;
            HostCommand::Identity
        }
        "clients" => {
            exactly(command, rest, [], true)?;
            HostCommand::Clients
        }
        "permissions" => match rest {
            [] => {
                return Err(Usage::wrong(
                    "permissions needs X25519: helio host permissions X25519 [LIST]",
                    true,
                ))
            }
            [x25519] => HostCommand::Permissions {
                x25519: x25519.clone(),
                set: None,
            },
            [x25519, list] => HostCommand::Permissions {
                x25519: x25519.clone(),
                set: Some(permissions(list)?),
            },
            [_, _, extra, ..] => {
                return Err(unexpected(extra, "permissions X25519 LIST"));
            }
        },
        "add-client" => {
            let names = ["NAME", "X25519", "MLKEM"];
            let (keys, flags) = rest.split_at(rest.len().min(3));
            if let Some(flag) = keys.iter().find(|k| k.starts_with("--")) {
                return Err(Usage::wrong(
                    format!(
                        "{flag} comes after the keys: helio host add-client NAME X25519 MLKEM \
                            [--agent] [--permissions LIST]"
                    ),
                    true,
                ));
            }
            let [name, x25519, mlkem] = exactly(command, keys, names, true)?;
            let (mut agent, mut chosen) = (false, None);
            let mut it = flags.iter();
            while let Some(flag) = it.next() {
                match flag.as_str() {
                    "--agent" => agent = true,
                    "--permissions" => match it.next() {
                        Some(list) => chosen = Some(permissions(list)?),
                        None => {
                            return Err(Usage::wrong(
                                "--permissions needs a LIST: all, see-only, view,keyboard,…",
                                true,
                            ))
                        }
                    },
                    _ => {
                        return Err(unexpected(
                            flag,
                            "add-client NAME X25519 MLKEM (--agent, --permissions LIST)",
                        ))
                    }
                }
            }
            HostCommand::AddClient {
                name,
                x25519,
                mlkem,
                agent,
                permissions: chosen,
            }
        }
        "remove-client" => {
            let [x25519] = exactly(command, rest, ["X25519"], true)?;
            HostCommand::RemoveClient { x25519 }
        }
        "pair" => {
            let usage = "pair PIN (--device NAME, --permissions LIST)";
            let (pin, flags) = rest.split_at(rest.len().min(1));
            let [pin] = exactly(command, pin, ["PIN"], true)?;
            if pin.starts_with("--") {
                return Err(Usage::wrong(
                    format!("{pin} comes after the PIN: helio host {usage}"),
                    true,
                ));
            }
            let (mut device, mut chosen) = (None, None);
            let mut it = flags.iter();
            while let Some(flag) = it.next() {
                let value = |it: &mut std::slice::Iter<String>| {
                    it.next()
                        .cloned()
                        .ok_or_else(|| Usage::wrong(format!("{flag} needs a value"), true))
                };
                match flag.as_str() {
                    "--device" => device = Some(value(&mut it)?),
                    "--permissions" => chosen = Some(permissions(&value(&mut it)?)?),
                    _ => return Err(unexpected(flag, usage)),
                }
            }
            HostCommand::Pair {
                pin,
                device,
                permissions: chosen,
            }
        }
        other => {
            return Err(Usage::wrong(
                format!("unknown command {other:?} for helio host"),
                true,
            ));
        }
    })
}

/// A number a flag was given.
fn number<T: std::str::FromStr + PartialOrd + Default>(
    flag: &str,
    text: &str,
) -> Result<T, String> {
    text.parse::<T>()
        .ok()
        .filter(|n| *n > T::default())
        .ok_or_else(|| format!("{flag} {text}: not a number above 0"))
}

/// The stream `flags` ask for: from this display's mode (`native`, fitted
/// to a standard aspect ratio as the app fits it) at 60 fps and Moonlight's
/// bitrate for the mode, as far as the flags do not say otherwise. `watch`:
/// watching an agent, with no sound (Ctrl+Alt+Shift+T takes over).
pub fn request(
    flags: &[String],
    native: &NativeMode,
    watch: bool,
) -> Result<StreamRequest, String> {
    let mut r = StreamRequest {
        width: native.width,
        height: native.height,
        fps: 60,
        mute_in_background: false,
        ..StreamRequest::default()
    };
    let mut mbps = None;
    let mut it = flags.iter();
    while let Some(flag) = it.next() {
        let flag = flag.as_str();
        let mut value = |what: &str| {
            it.next()
                .map(String::as_str)
                .ok_or_else(|| format!("{flag} needs {what}"))
        };
        match flag {
            "--size" => {
                let size = value("a size, WxH (1920x1080)")?;
                let (w, h) = size
                    .split_once('x')
                    .ok_or_else(|| format!("--size {size}: not WxH (1920x1080)"))?;
                r.width = number("--size", w)?;
                r.height = number("--size", h)?;
            }
            "--fps" => r.fps = number(flag, value("a frame rate")?)?,
            "--mbps" => mbps = Some(number::<u32>(flag, value("a bitrate, in Mbit/s")?)?),
            "--codec" => {
                r.codecs = match value("a codec: h264, hevc or av1")? {
                    "h264" => codec::H264,
                    "hevc" => codec::HEVC,
                    "av1" => codec::AV1,
                    other => return Err(format!("--codec {other}: h264, hevc or av1")),
                }
            }
            "--audio-channels" => {
                r.audio_channels = match value("2, 6 or 8")? {
                    "2" => 2,
                    "6" => 6,
                    "8" => 8,
                    other => {
                        return Err(format!(
                            "--audio-channels {other}: 2 (stereo), 6 (5.1) or 8 (7.1)"
                        ))
                    }
                }
            }
            "--via" => {
                let addr = value("an address and port (192.168.1.20:47800)")?;
                r.via.push(addr.parse().map_err(|_| {
                    format!("--via {addr}: not an address and port (192.168.1.20:47800)")
                })?);
            }
            "--hdr" => r.hdr = true,
            "--yuv444" => r.yuv444 = true,
            "--windowed" => r.fullscreen = false,
            "--no-vsync" => r.vsync = false,
            "--frame-pacing" => r.frame_pacing = true,
            "--stats" => r.show_stats = true,
            "--cmd-is-win" => r.cmd_is_win = true,
            "--mute-in-background" => r.mute_in_background = true,
            "--no-audio" => r.audio_channels = 0,
            "--host-audio" => r.host_audio = true,
            "--steam" => r.app = app::STEAM_BIG_PICTURE,
            "--keep-host-displays" => r.keep_host_displays = true,
            "--no-clipboard" => r.clipboard = false,
            "--wan-only" => r.wan_only = true,
            other => return Err(format!("unknown flag {other}")),
        }
    }
    if watch {
        r.watch = true;
        r.audio_channels = 0;
    }
    r.bitrate_kbps = match mbps {
        Some(m) => m.saturating_mul(1000),
        None => helio_client::stream::default_bitrate_kbps(r.width as u32, r.height as u32, r.fps),
    };
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    fn read(line: &str) -> Result<Command, Usage> {
        parse(&words(line))
    }

    fn host(line: &str) -> HostCommand {
        match read(line) {
            Ok(Command::Host(c)) => c,
            other => panic!("{line}: {other:?}"),
        }
    }

    /// Why `line` was not taken, and whether the host's help goes with it.
    fn refused(line: &str) -> (String, bool) {
        match read(line) {
            Err(Usage {
                why: Some(why),
                host,
            }) => (why, host),
            other => panic!("{line}: {other:?}"),
        }
    }

    const NATIVE: NativeMode = NativeMode {
        width: 2560,
        height: 1440,
        max_fps: 120,
        desktop: None,
    };

    fn stream(flags: &str) -> Result<StreamRequest, String> {
        request(&words(flags), &NATIVE, false)
    }

    #[test]
    fn each_client_command_reads_its_words() {
        assert_eq!(read("identity"), Ok(Command::Identity));
        assert_eq!(read("discover"), Ok(Command::Discover));
        assert_eq!(read("hosts"), Ok(Command::Hosts));
        assert_eq!(
            read("pair 192.168.1.20:47801"),
            Ok(Command::Pair {
                address: "192.168.1.20:47801".into()
            })
        );
        assert_eq!(
            read("pair-agent gaming-pc"),
            Ok(Command::PairAgent {
                address: "gaming-pc".into()
            })
        );
        assert_eq!(
            read("add-host gaming-pc 192.168.1.20 X M"),
            Ok(Command::AddHost {
                name: "gaming-pc".into(),
                address: "192.168.1.20".into(),
                x25519: "X".into(),
                mlkem: "M".into()
            })
        );
        assert_eq!(
            read("remove-host gaming-pc"),
            Ok(Command::RemoveHost {
                name: "gaming-pc".into()
            })
        );
        assert_eq!(
            read("wake gaming-pc"),
            Ok(Command::Wake {
                name: "gaming-pc".into()
            })
        );
        assert_eq!(
            read("stream gaming-pc --fps 120 --stats"),
            Ok(Command::Stream {
                name: "gaming-pc".into(),
                flags: words("--fps 120 --stats"),
                watch: false
            })
        );
        assert_eq!(
            read("watch gaming-pc"),
            Ok(Command::Stream {
                name: "gaming-pc".into(),
                flags: vec![],
                watch: true
            })
        );
    }

    #[test]
    fn each_host_command_reads_its_words() {
        assert_eq!(host("host enable"), HostCommand::Enable);
        assert_eq!(host("host disable"), HostCommand::Disable);
        assert_eq!(host("host restart"), HostCommand::Restart);
        assert_eq!(
            host("host pair 4821"),
            HostCommand::Pair {
                pin: "4821".into(),
                device: None,
                permissions: None,
            }
        );
        assert_eq!(
            host("host pair 4821 --device phone --permissions see-only"),
            HostCommand::Pair {
                pin: "4821".into(),
                device: Some("phone".into()),
                permissions: Some(Permissions::parse("see-only").unwrap()),
            }
        );
        assert_eq!(host("host status"), HostCommand::Status { json: false });
        assert_eq!(
            host("host status --json"),
            HostCommand::Status { json: true }
        );
        assert_eq!(host("host identity"), HostCommand::Identity);
        assert_eq!(host("host clients"), HostCommand::Clients);
        assert_eq!(
            host("host permissions KEY"),
            HostCommand::Permissions {
                x25519: "KEY".into(),
                set: None
            }
        );
        assert_eq!(
            host("host permissions KEY view,keyboard,mouse"),
            HostCommand::Permissions {
                x25519: "KEY".into(),
                set: Some(Permissions::parse("mouse,keyboard,view").unwrap())
            }
        );
        assert_eq!(
            host("host remove-client KEY"),
            HostCommand::RemoveClient {
                x25519: "KEY".into()
            }
        );
    }

    #[test]
    fn add_client_takes_its_flags_after_the_keys_in_any_order() {
        let plain = HostCommand::AddClient {
            name: "laptop".into(),
            x25519: "X".into(),
            mlkem: "M".into(),
            agent: false,
            permissions: None,
        };
        assert_eq!(host("host add-client laptop X M"), plain);
        let agent = HostCommand::AddClient {
            name: "agent".into(),
            x25519: "X".into(),
            mlkem: "M".into(),
            agent: true,
            permissions: Some(Permissions::parse("view").unwrap()),
        };
        assert_eq!(
            host("host add-client agent X M --agent --permissions see-only"),
            agent
        );
        assert_eq!(
            host("host add-client agent X M --permissions see-only --agent"),
            agent
        );
        assert!(refused("host add-client --agent agent X M")
            .0
            .contains("comes after the keys"));
        assert!(refused("host add-client laptop X M --permissions")
            .0
            .contains("needs a LIST"));
        assert!(refused("host add-client laptop X M --admin")
            .0
            .contains("\"--admin\""));
    }

    #[test]
    fn help_is_asked_for_in_every_usual_way() {
        for line in ["--help", "-h", "help", "stream gaming-pc --help", "pair -h"] {
            assert_eq!(read(line), Ok(Command::Help { host: false }), "{line}");
        }
        for line in [
            "host --help",
            "host -h",
            "host help",
            "help host",
            "--help host",
            "host add-client --help",
        ] {
            assert_eq!(read(line), Ok(Command::Help { host: true }), "{line}");
        }
    }

    #[test]
    fn nothing_asked_for_is_answered_with_the_help() {
        assert_eq!(
            read(""),
            Err(Usage {
                why: None,
                host: false
            })
        );
        assert_eq!(
            read("host"),
            Err(Usage {
                why: None,
                host: true
            })
        );
    }

    #[test]
    fn a_word_missing_or_left_over_is_named() {
        assert_eq!(
            refused("pair").0,
            "pair needs HOST[:PORT]: helio pair HOST[:PORT]"
        );
        assert_eq!(
            refused("add-host gaming-pc 192.168.1.20").0,
            "add-host needs X25519: helio add-host NAME ADDR X25519 MLKEM"
        );
        assert_eq!(
            refused("identity now").0,
            "unexpected \"now\" after helio identity"
        );
        assert_eq!(
            refused("host remove-client"),
            (
                "remove-client needs X25519: helio host remove-client X25519".into(),
                true
            )
        );
        assert_eq!(
            refused("host enable now"),
            ("unexpected \"now\" after helio host enable".into(), true)
        );
        assert!(refused("host status --xml").0.contains("\"--xml\""));
        assert!(refused("host permissions KEY all extra")
            .0
            .contains("\"extra\""));
        assert!(refused("host permissions").0.contains("needs X25519"));
        assert_eq!(
            refused("frobnicate"),
            ("unknown command \"frobnicate\"".into(), false)
        );
        assert_eq!(
            refused("host frobnicate"),
            ("unknown command \"frobnicate\" for helio host".into(), true)
        );
    }

    #[test]
    fn a_permission_not_known_is_refused_not_dropped() {
        let (why, host) = refused("host permissions KEY view,teleport");
        assert!(why.contains("\"teleport\"") && host, "{why}");
        assert!(
            refused("host add-client laptop X M --permissions everything")
                .0
                .contains("\"everything\"")
        );
    }

    #[test]
    fn a_stream_needs_a_name_before_its_flags() {
        assert!(refused("stream").0.contains("needs the NAME"));
        assert!(refused("stream --fps 60").0.contains("needs the NAME"));
        assert!(refused("watch").0.contains("needs the NAME"));
    }

    #[test]
    fn a_stream_flag_misspelt_or_without_its_value_is_refused_before_connecting() {
        assert_eq!(refused("stream pc --fsp 60").0, "unknown flag --fsp");
        assert_eq!(stream("--fps").unwrap_err(), "--fps needs a frame rate");
        assert!(stream("--fps 0").unwrap_err().contains("above 0"));
        assert!(stream("--size 1920").unwrap_err().contains("not WxH"));
        assert!(stream("--size 1920x").unwrap_err().contains("--size"));
        assert!(stream("--size 99999x1080").unwrap_err().contains("--size"));
        assert!(stream("--codec vp9")
            .unwrap_err()
            .contains("h264, hevc or av1"));
        assert!(stream("--audio-channels 4")
            .unwrap_err()
            .contains("2 (stereo)"));
        assert!(stream("--via gaming-pc")
            .unwrap_err()
            .contains("not an address"));
        assert!(stream("--mbps lots").unwrap_err().contains("--mbps"));
    }

    #[test]
    fn a_stream_without_flags_is_this_display_at_60_fps_and_moonlights_bitrate() {
        let r = stream("").unwrap();
        assert_eq!((r.width, r.height, r.fps), (2560, 1440, 60));
        assert_eq!(
            r.bitrate_kbps,
            helio_client::stream::default_bitrate_kbps(2560, 1440, 60)
        );
        assert!(r.fullscreen && r.vsync && r.clipboard && !r.watch);
        assert_eq!(r.audio_channels, 2);
        // The CLI's window is its own: in the background, it keeps its sound.
        assert!(!r.mute_in_background);
    }

    #[test]
    fn each_stream_flag_sets_what_it_says() {
        let r = stream("--size 1280x720 --fps 120").unwrap();
        assert_eq!((r.width, r.height, r.fps), (1280, 720, 120));
        // The bitrate follows the mode asked for.
        assert_eq!(
            r.bitrate_kbps,
            helio_client::stream::default_bitrate_kbps(1280, 720, 120)
        );
        assert_eq!(stream("--mbps 80").unwrap().bitrate_kbps, 80_000);
        assert_eq!(stream("--codec h264").unwrap().codecs, codec::H264);
        assert_eq!(stream("--codec hevc").unwrap().codecs, codec::HEVC);
        assert_eq!(stream("--codec av1").unwrap().codecs, codec::AV1);
        assert_eq!(stream("--audio-channels 8").unwrap().audio_channels, 8);
        assert_eq!(stream("--no-audio").unwrap().audio_channels, 0);
        assert_eq!(stream("--steam").unwrap().app, app::STEAM_BIG_PICTURE);
        let via = stream("--via 192.168.1.20:47800 --via [2001:db8::1]:47800").unwrap();
        assert_eq!(via.via.len(), 2);
        let r = stream(
            "--hdr --yuv444 --windowed --no-vsync --frame-pacing --stats --cmd-is-win \
             --mute-in-background --host-audio --keep-host-displays --no-clipboard --wan-only",
        )
        .unwrap();
        assert!(r.hdr && r.yuv444 && !r.fullscreen && !r.vsync && r.frame_pacing);
        assert!(r.show_stats && r.cmd_is_win && r.mute_in_background && r.host_audio);
        assert!(r.keep_host_displays && !r.clipboard && r.wan_only);
    }

    #[test]
    fn watching_an_agent_has_no_sound_whatever_the_flags() {
        let r = request(&words("--audio-channels 6"), &NATIVE, true).unwrap();
        assert!(r.watch);
        assert_eq!(r.audio_channels, 0);
    }

    /// Every flag `request` takes, with a value where it needs one.
    const FLAGS: [&str; 20] = [
        "--size 1280x720",
        "--fps 60",
        "--mbps 20",
        "--codec h264",
        "--hdr",
        "--yuv444",
        "--windowed",
        "--no-vsync",
        "--frame-pacing",
        "--stats",
        "--cmd-is-win",
        "--mute-in-background",
        "--audio-channels 2",
        "--no-audio",
        "--host-audio",
        "--steam",
        "--keep-host-displays",
        "--no-clipboard",
        "--wan-only",
        "--via 192.168.1.20:47800",
    ];

    #[test]
    fn the_help_lists_every_command_and_flag() {
        let (help, host_help) = (
            crate::help::text(false, false),
            crate::help::text(true, false),
        );
        for flag in FLAGS {
            assert!(stream(flag).is_ok(), "{flag}");
            let name = flag.split(' ').next().unwrap();
            assert!(
                help.contains(&format!("  {name} ")),
                "{name} is not in the help"
            );
        }
        for command in [
            "stream NAME",
            "watch NAME",
            "wake NAME",
            "discover",
            "pair HOST[:PORT]",
            "pair-agent HOST[:PORT]",
            "hosts",
            "add-host NAME ADDR X25519 MLKEM",
            "remove-host NAME",
            "identity",
        ] {
            assert!(help.contains(&format!("  {command}")), "{command}");
        }
        for command in [
            "enable",
            "disable",
            "status [--json]",
            "identity",
            "clients",
            "permissions X25519",
            "add-client NAME X25519 MLKEM [--agent] [--permissions LIST]",
            "remove-client X25519",
        ] {
            assert!(help.contains(&format!("  host {command}")), "{command}");
            assert!(host_help.contains(&format!("  {command}")), "{command}");
        }
    }
}
