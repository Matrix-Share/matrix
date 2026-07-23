//! Interface capabilities — the self-description each transport publishes so the
//! engine and router can treat any medium uniformly (PRD §8.4 `caps`).
//!
//! MTUs and ranges below mirror the honest figures from the spectrum design doc
//! (Tier 1 app-native radios + transducers, Tier 2 gateway peripherals).

use serde::{Deserialize, Serialize};

/// Which physical medium an interface drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterfaceKind {
    /// Bluetooth LE mesh — the primary app-native transport.
    Ble,
    /// Wi-Fi Aware (NAN) / Wi-Fi Direct — highest app-native bandwidth.
    WifiAware,
    /// Ultra-wideband — short range + precise ranging.
    Uwb,
    /// NFC — tap-to-pair / key exchange.
    Nfc,
    /// Data-over-sound (ggwave/Quiet) — works when radios are off/jammed.
    Ultrasound,
    /// Streaming/animated QR (screen ↔ camera) — air-gap friendly optical.
    OpticalQr,
    /// Flashlight ↔ camera/light-sensor beacon.
    Flashlight,
    /// LoRa via a paired RNode/Meshtastic device (gateway peripheral).
    LoRa,
    /// Internet uplink (TCP/QUIC to a cloud relay / other gateways).
    Internet,
    /// In-process test medium.
    Memory,
}

/// Coarse throughput bucket (spectrum doc columns).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ThroughputClass {
    VeryLow,
    Low,
    Medium,
    High,
}

/// Coarse range bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RangeClass {
    /// Centimetres (NFC).
    Touch,
    /// A room / same building (sound, optical, BLE close).
    Room,
    /// Tens–hundreds of metres (BLE/Wi-Fi Aware).
    Local,
    /// Kilometres (LoRa).
    Long,
    /// Effectively global (internet).
    Global,
}

/// A transport's self-description. The router reads `mtu` (for fragmentation)
/// and `bridges_offmesh` (gateway routing); compliance code reads `region` /
/// `max_power_mw` to enforce lawful radio operation (CR-1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterfaceCaps {
    pub kind: InterfaceKind,
    pub name: String,
    /// Max transmittable frame size in bytes. Drives fragmentation.
    pub mtu: usize,
    pub throughput: ThroughputClass,
    pub range: RangeClass,
    pub full_duplex: bool,
    /// True if this interface can bridge the mesh off-cluster (LoRa/internet),
    /// i.e. a gateway-capable link (FR-37).
    pub bridges_offmesh: bool,
    /// Regulatory region for radio interfaces (e.g. "IN865"); enforced by
    /// compliance checks (CR-1). `None` for non-radio (sound/light) links.
    pub region: Option<String>,
    /// Transmit power cap in milliwatts, where regulated (LoRa ≤ 1 W in IN865).
    pub max_power_mw: Option<u32>,
}

impl InterfaceCaps {
    /// Usable payload per frame after the fragmentation header (see [`crate::frame`]).
    pub fn usable_mtu(&self) -> usize {
        self.mtu.saturating_sub(crate::frame::FRAME_HEADER_BYTES)
    }

    /// Bluetooth LE (ATT-ish MTU ~244 B), room/local range, primary transport.
    pub fn ble() -> Self {
        Self {
            kind: InterfaceKind::Ble,
            name: "ble".into(),
            mtu: 244,
            throughput: ThroughputClass::Low,
            range: RangeClass::Local,
            full_duplex: true,
            bridges_offmesh: false,
            region: None,
            max_power_mw: None,
        }
    }

    /// Wi-Fi Aware / Direct — high bandwidth, larger MTU.
    pub fn wifi_aware() -> Self {
        Self {
            kind: InterfaceKind::WifiAware,
            name: "wifi-aware".into(),
            mtu: 1200,
            throughput: ThroughputClass::High,
            range: RangeClass::Local,
            full_duplex: true,
            bridges_offmesh: false,
            region: None,
            max_power_mw: None,
        }
    }

    /// Data-over-sound (near-ultrasonic). Tiny MTU — the acid test for
    /// fragmentation (Dhwani, SIGCOMM 2013).
    pub fn ultrasound() -> Self {
        Self {
            kind: InterfaceKind::Ultrasound,
            name: "ultrasound".into(),
            mtu: 128,
            throughput: ThroughputClass::VeryLow,
            range: RangeClass::Room,
            full_duplex: false,
            bridges_offmesh: false,
            region: None,
            max_power_mw: None,
        }
    }

    /// Streaming-QR optical link (screen ↔ camera).
    pub fn optical_qr() -> Self {
        Self {
            kind: InterfaceKind::OpticalQr,
            name: "optical-qr".into(),
            mtu: 1024,
            throughput: ThroughputClass::Low,
            range: RangeClass::Room,
            full_duplex: false,
            bridges_offmesh: false,
            region: None,
            max_power_mw: None,
        }
    }

    /// LoRa gateway link in the Indian ISM band, power-capped (CR-1).
    pub fn lora_in865() -> Self {
        Self {
            kind: InterfaceKind::LoRa,
            name: "lora-in865".into(),
            mtu: 222, // SF7 max application payload
            throughput: ThroughputClass::VeryLow,
            range: RangeClass::Long,
            full_duplex: false,
            bridges_offmesh: true,
            region: Some("IN865".into()),
            max_power_mw: Some(1000), // ≤ 1 W
        }
    }

    /// Internet uplink — large MTU, global reach, gateway-capable.
    pub fn internet() -> Self {
        Self {
            kind: InterfaceKind::Internet,
            name: "internet".into(),
            mtu: 65_535,
            throughput: ThroughputClass::High,
            range: RangeClass::Global,
            full_duplex: true,
            bridges_offmesh: true,
            region: None,
            max_power_mw: None,
        }
    }

    /// Verify a radio interface is configured for a lawful region/power (CR-1).
    /// Non-radio links (sound/light) are always compliant.
    pub fn is_lawful_radio(&self) -> bool {
        match self.kind {
            InterfaceKind::LoRa => {
                self.region.as_deref() == Some("IN865")
                    && self.max_power_mw.is_some_and(|p| p <= 1000)
            }
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lora_enforces_region_and_power() {
        assert!(InterfaceCaps::lora_in865().is_lawful_radio());
        let mut bad = InterfaceCaps::lora_in865();
        bad.max_power_mw = Some(2000);
        assert!(!bad.is_lawful_radio());
        let mut wrong_region = InterfaceCaps::lora_in865();
        wrong_region.region = Some("EU868".into());
        assert!(!wrong_region.is_lawful_radio());
    }

    #[test]
    fn usable_mtu_smaller_than_mtu() {
        assert!(InterfaceCaps::ultrasound().usable_mtu() < InterfaceCaps::ultrasound().mtu);
        assert!(InterfaceCaps::internet().usable_mtu() > InterfaceCaps::ble().usable_mtu());
    }
}
