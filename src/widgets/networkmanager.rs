use crate::color::Color;
use crate::draw::{Font, ROBOTO_REGULAR};
use crate::widget::{DrawContext, DrawReport, KeyState, ModifiersState, WaitContext, Widget};
use crate::cmd::Cmd;

use std::sync::mpsc::Sender;
use std::cell::RefCell;
use std::net::Ipv4Addr;
use std::fmt;

use nix::poll::{PollFd, PollFlags};

use dbus;


#[derive(Debug)]
enum NetworkState {
    Unknown = 0,
    Asleep = 10,
    Disconnected = 20,
    Disconnecting = 30,
    Connecting = 40,
    ConnectedLocal = 50,
    ConnectedSite = 60,
    ConnectedGlobal = 70,
}

impl From<u32> for NetworkState {
    fn from(id: u32) -> Self {
        match id {
            // https://developer.gnome.org/NetworkManager/unstable/nm-dbus-types.html#NMState
            10 => NetworkState::Asleep,
            20 => NetworkState::Disconnected,
            30 => NetworkState::Disconnecting,
            40 => NetworkState::Connecting,
            50 => NetworkState::ConnectedLocal,
            60 => NetworkState::ConnectedSite,
            70 => NetworkState::ConnectedGlobal,
            _  => NetworkState::Unknown,
        }
    }
}

enum ActiveConnectionState {
    Unknown,
    Activating,
    Activated,
    Deactivating,
    Deactivated,
}

impl From<u32> for ActiveConnectionState {
    fn from(id: u32) -> Self {
        match id {
            // https://developer.gnome.org/NetworkManager/stable/nm-dbus-types.html#NMActiveConnectionState
            1 => ActiveConnectionState::Activating,
            2 => ActiveConnectionState::Activated,
            3 => ActiveConnectionState::Deactivating,
            4 => ActiveConnectionState::Deactivated,
            _ => ActiveConnectionState::Unknown,
        }
    }
}

#[derive(Debug)]
enum DeviceType {
    Unknown,
    Ethernet,
    Wifi,
    Modem,
    Bridge,
    TUN,
    Wireguard,
}

impl From<u32> for DeviceType {
    fn from(id: u32) -> Self {
        match id {
            // https://developer.gnome.org/NetworkManager/stable/nm-dbus-types.html#NMDeviceType
            1 => DeviceType::Ethernet,
            2 => DeviceType::Wifi,
            8 => DeviceType::Modem,
            13 => DeviceType::Bridge,
            16 => DeviceType::TUN,
            29 => DeviceType::Wireguard,
            _ => DeviceType::Unknown,
        }
    }
}

#[derive(Debug)]
struct Ipv4Address {
    address: Ipv4Addr,
    prefix: u32,
    gateway: Ipv4Addr,
}

trait ByteOrderSwap {
    fn swap(&self) -> Self;
}

impl ByteOrderSwap for u32 {
    fn swap(&self) -> u32 {
        ((self & 0x000000FF) << 24) | ((self & 0x0000FF00) << 8) | ((self & 0x00FF0000) >> 8) | ((self & 0xFF000000) >> 24)
    }
}

impl<'a> From<dbus::arg::Array<'a, u32, dbus::arg::Iter<'a>>> for Ipv4Address {
    fn from(s: dbus::arg::Array<'a, u32, dbus::arg::Iter<'a>>) -> Ipv4Address {
        let mut i = s.into_iter();
        Ipv4Address {
            address: Ipv4Addr::from(i.next().unwrap().swap()),
            prefix: i.next().unwrap(),
            gateway: Ipv4Addr::from(i.next().unwrap().swap()),
        }
    }
}

impl fmt::Display for Ipv4Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.address, self.prefix)
    }
}

fn get_nm_property<'a>(
    con: &dbus::Connection,
    obj: &str,
    path: &dbus::Path<'a>,
    property: &str,
) -> Result<dbus::Message, ::std::io::Error> {
    let msg = dbus::Message::new_method_call(
        obj,
        path,
        "org.freedesktop.DBus.Properties",
        "Get",
    )
    .map_err(|_| {
        ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not send make dbus method call",
        )
    })?
    .append2(
        dbus::MessageItem::Str("org.freedesktop.NetworkManager".to_string()),
        dbus::MessageItem::Str(property.to_string()),
    );

    con.send_with_reply_and_block(msg, 1000).map_err(|_| {
        ::std::io::Error::new(::std::io::ErrorKind::Other, "could not send dbus message")
    })
}

fn state(con: &dbus::Connection) -> Result<NetworkState, ::std::io::Error> {
    let m = get_nm_property(con, "org.freedesktop.NetworkManager", &"/org/freedesktop/NetworkManager".into(), "State").map_err(|_| ::std::io::Error::new(
        ::std::io::ErrorKind::Other,
        "could not retrieve state",
    ))?;
    let state: dbus::arg::Variant<u32> = m.get1().ok_or_else(|| ::std::io::Error::new(
        ::std::io::ErrorKind::Other,
        "could not read property",
    ))?;
    Ok(NetworkState::from(state.0))
}

fn primary_connection(con: &dbus::Connection) -> Result<NmConnection, ::std::io::Error> {
    let m = get_nm_property(con, "org.freedesktop.NetworkManager", &"/org/freedesktop/NetworkManager".into(), "PrimaryConnection").map_err(|_| ::std::io::Error::new(
        ::std::io::ErrorKind::Other,
        "could not primary connection",
    ))?;
    let primary_connection: dbus::arg::Variant<dbus::Path> = m.get1().ok_or_else(|| ::std::io::Error::new(
        ::std::io::ErrorKind::Other,
        "could not read property",
    ))?;
    if let Ok(conn) = primary_connection.0.as_cstr().to_str() {
        if conn == "/" {
            return Err(::std::io::Error::new(
                ::std::io::ErrorKind::Other,
                "no primary connection",
            ));
        }
    }
    Ok(NmConnection { path: primary_connection.0.clone() })
}

fn active_connections(con: &dbus::Connection) -> Result<Vec<NmConnection>, ::std::io::Error> {
    let m = get_nm_property(con, "org.freedesktop.NetworkManager", &"/org/freedesktop/NetworkManager".into(), "ActiveConnections").map_err(|_| ::std::io::Error::new(
        ::std::io::ErrorKind::Other,
        "could not retrieve active connections",
    ))?;
    let active_connections: dbus::arg::Variant<dbus::arg::Array<dbus::Path, dbus::arg::Iter>> = m.get1().ok_or_else(|| ::std::io::Error::new(
        ::std::io::ErrorKind::Other,
        "could not read property",
    ))?;
    Ok(active_connections.0.into_iter().map(|x| NmConnection { path: x }).collect())
}

#[derive(Clone)]
struct NmConnection<'a> {
    path: dbus::Path<'a>,
}

impl<'a> NmConnection<'a> {
    fn state(&self, con: &dbus::Connection) -> Result<ActiveConnectionState, ::std::io::Error> {
        let m = get_nm_property(con, "org.freedesktop.NetworkManager.Connection.Active", &self.path, "State").map_err(|_| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not retrieve connection state",
        ))?;
        let state: dbus::arg::Variant<u32> = m.get1().ok_or_else(|| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not read property",
        ))?;
        Ok(ActiveConnectionState::from(state.0))
    }

    fn id(&self, con: &dbus::Connection) -> Result<String, ::std::io::Error> {
        let m = get_nm_property(con, "org.freedesktop.NetworkManager.Connection.Active", &self.path, "Id").map_err(|_| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not retrieve connection ID",
        ))?;
        let id: dbus::arg::Variant<String> = m.get1().ok_or_else(|| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not read property",
        ))?;
        Ok(id.0)
    }

    fn devices(&self, con: &dbus::Connection) -> Result<Vec<NmDevice>, ::std::io::Error> {
        let m = get_nm_property(con, "org.freedesktop.NetworkManager.Connection.Active", &self.path, "Devices").map_err(|_| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not retrieve connection devices",
        ))?;
        let devices: dbus::arg::Variant<dbus::arg::Array<dbus::Path, dbus::arg::Iter>> = m.get1().ok_or_else(|| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not read property",
        ))?;
        Ok(devices.0.into_iter().map(|x| NmDevice { path: x }).collect())
    }
}

#[derive(Clone)]
struct NmDevice<'a> {
    path: dbus::Path<'a>,
}

impl<'a> NmDevice<'a> {
    fn device_type(&self, con: &dbus::Connection) -> Result<DeviceType, ::std::io::Error> {
        let m = get_nm_property(con, "org.freedesktop.NetworkManager.Device", &self.path, "DeviceType").map_err(|_| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not retrieve device type",
        ))?;

        let device_type: dbus::arg::Variant<u32> = m.get1().ok_or_else(|| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not read property",
        ))?;
        Ok(DeviceType::from(device_type.0))
    }

    fn ip4config(&self, con: &dbus::Connection) -> Result<NmIp4Config, ::std::io::Error> {
        let m = get_nm_property(con, "org.freedesktop.NetworkManager.Device", &self.path, "Ip4Config").map_err(|_| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not retrieve ip4config",
        ))?;

        let ip4config: dbus::arg::Variant<dbus::Path> = m.get1().ok_or_else(|| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not read property",
        ))?;
        Ok(NmIp4Config { path: ip4config.0 })
    }

    fn active_access_point(&self, con: &dbus::Connection) -> Result<NmAccessPoint, ::std::io::Error> {
        let m = get_nm_property(con, "org.freedesktop.NetworkManager.Device.Wireless", &self.path, "ActiveAccessPoint").map_err(|_| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not retrieve device type",
        ))?;

        let active_ap: dbus::arg::Variant<dbus::Path> = m.get1().ok_or_else(|| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not read property",
        ))?;
        Ok(NmAccessPoint { path: active_ap.0 })
    }
}

#[derive(Clone)]
struct NmAccessPoint<'a> {
    path: dbus::Path<'a>,
}

impl<'a> NmAccessPoint<'a> {
    fn ssid(&self, con: &dbus::Connection) -> Result<String, ::std::io::Error> {
        let m = get_nm_property(con, "org.freedesktop.NetworkManager.AccessPoint", &self.path, "Ssid").map_err(|_| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not retrieve ssid",
        ))?;

        let ssid: dbus::arg::Variant<dbus::arg::Array<u8, dbus::arg::Iter>> = m.get1().ok_or_else(|| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not read property",
        ))?;
        Ok(std::str::from_utf8(&ssid.0.into_iter().collect::<Vec<u8>>())
            .map_err(|_| ::std::io::Error::new(
                ::std::io::ErrorKind::Other,
                "could not parse ssid",
            ))?
            .to_string())
    }

    fn strength(&self, con: &dbus::Connection) -> Result<u8, ::std::io::Error> {
        let m = get_nm_property(con, "org.freedesktop.NetworkManager.AccessPoint", &self.path, "Strength").map_err(|_| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not retrieve access point strength",
        ))?;

        let strength: dbus::arg::Variant<u8> = m.get1().ok_or_else(|| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not read property",
        ))?;
        Ok(strength.0)
    }

    fn frequency(&self, con: &dbus::Connection) -> Result<u32, ::std::io::Error> {
        let m = get_nm_property(con, "org.freedesktop.NetworkManager.AccessPoint", &self.path, "Frequency").map_err(|_| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not retrieve access point frequency",
        ))?;

        let frequency: dbus::arg::Variant<u32> = m.get1().ok_or_else(|| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not read property",
        ))?;
        Ok(frequency.0)
    }
}

#[derive(Clone)]
struct NmIp4Config<'a> {
    path: dbus::Path<'a>,
}

impl<'a> NmIp4Config<'a> {
    fn addresses(&self, con: &dbus::Connection) -> Result<Vec<Ipv4Address>, ::std::io::Error> {
        let m = get_nm_property(con, "org.freedesktop.NetworkManager.IP4Config", &self.path, "Addresses").map_err(|_| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not retrieve addresses",
        ))?;

        let addresses: dbus::arg::Variant<dbus::arg::Array<dbus::arg::Array<u32, dbus::arg::Iter>, dbus::arg::Iter>> = m.get1().ok_or_else(|| ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not read property",
        ))?;
        Ok(addresses.0.into_iter().map(|addr| Ipv4Address::from(addr)).collect())
    }
}

// The widget is created on a different thread than where it will be used, so
// we need it to be Send. However, dbus::Connection has a void*, so it's not
// auto-derived. So, we make a small wrapper where we make it Send.
struct DbusConnection(dbus::Connection);

impl std::convert::AsRef<dbus::Connection> for DbusConnection {
    fn as_ref(&self) -> &dbus::Connection {
        return &self.0;
    }
}

unsafe impl Send for DbusConnection {}

#[derive(Debug)]
struct Device {
    // name: String,
    t: DeviceType,
    ips: Vec<Ipv4Address>,
}

struct Connection {
    devices: Vec<Device>,
}

pub struct NetworkManager {
    con: DbusConnection,
    watch: dbus::Watch,

    font: RefCell<Font>,
    font_size: u32,
    state: NetworkState,
    dirty: bool,
    tx: Sender<Cmd>,
    length: u32,
    connections: Vec<Connection>,
}

impl NetworkManager {
    fn update(&mut self) -> Result<(), ::std::io::Error> {
        self.state = state(self.con.as_ref())?;

        self.connections = active_connections(self.con.as_ref())?
            .into_iter()
            .map(|c| {
                let devices = c.devices(self.con.as_ref()).unwrap()
                    .into_iter()
                    .map(|d| {
                        let t = d.device_type(self.con.as_ref()).unwrap();
                        let ips = d.ip4config(self.con.as_ref()).unwrap().addresses(self.con.as_ref()).unwrap();
                        Device{
                            t,
                            ips,
                        }
                    })
                    .collect();
                Connection{
                    devices
                }
            })
            .collect();

        Ok(())
    }

    pub fn new(font_size: f32, length: u32, tx: Sender<Cmd>) -> Result<Box<NetworkManager>, ::std::io::Error> {
        let con = dbus::Connection::get_private(dbus::BusType::System).map_err(|_| {
            ::std::io::Error::new(::std::io::ErrorKind::Other, "unable to open dbus")
        })?;


        let font = Font::new(&ROBOTO_REGULAR, font_size);

        let fds = con.watch_fds();
        if fds.len() != 1 {
            return Err(::std::io::Error::new(
                ::std::io::ErrorKind::Other,
                "expected 1 watch fd from dbus",
            ));
        }

        let mut nm = NetworkManager {
            con: DbusConnection(con),
            font: RefCell::new(font),
            font_size: font_size as u32,
            length,
            state: NetworkState::Unknown,
            dirty: true,
            watch: fds[0],
            tx,
            connections: Vec::new(),
        };

        nm.update()?;

        Ok(Box::new(nm))
    }
}

impl Widget for NetworkManager {
    fn wait(&mut self, ctx: &mut WaitContext) {
        let mut stuff = false;
        for _ in self
            .con
            .as_ref()
            .watch_handle(self.watch.fd(), dbus::WatchEvent::Readable as u32)
        {
            stuff = true;
        }
        if stuff {
            self.update().unwrap();
            self.dirty = true;
            self.tx.send(Cmd::Draw).unwrap();
        }

        ctx.fds
            .push(PollFd::new(self.watch.fd(), PollFlags::POLLIN));
    }
    fn enter(&mut self) {}
    fn leave(&mut self) {}
    fn size(&self) -> (u32, u32) {
        (self.length, self.font_size * self.connections.len() as u32)
    }
    fn draw(
        &mut self,
        ctx: &mut DrawContext,
        pos: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        let (width, height) = self.size();
        {
            if !self.dirty && !ctx.force {
                return Ok(DrawReport::empty(width, height));
            }
            self.dirty = false;
        }

        let buf = &mut ctx.buf.subdimensions((pos.0, pos.1, width, height))?;
        buf.memset(ctx.bg);

        for con in &self.connections {
            if con.devices.len() == 0 {
                continue;
            }
            let d = &con.devices[0];
            let c = Color::new(1.0, 1.0, 1.0, 1.0);
            self.font.borrow_mut().auto_draw_text(buf, ctx.bg, &c, "network")?;
            let bar_off = 5 * self.font_size;
            self.font.borrow_mut().auto_draw_text(&mut buf.offset((bar_off, 0))?, ctx.bg, &c, &format!("{:?}", d))?;
        }
        Ok(DrawReport {
            width: width,
            height: height,
            damage: vec![buf.get_signed_bounds()],
            full_damage: false,
        })
    }

    fn keyboard_input(&mut self, _: u32, _: ModifiersState, _: KeyState, _: Option<String>) {}
    fn mouse_click(&mut self, _: u32, _: (u32, u32)) {}
    fn mouse_scroll(&mut self, _: (f64, f64), _: (u32, u32)) {}
}
