//! TimG Group driver.

use kernel::hil::time::{self, Alarm, Counter, Ticks, Ticks64, Time};
use kernel::utilities::cells::OptionalCell;
use kernel::utilities::registers::interfaces::{ReadWriteable, Readable, Writeable};
use kernel::utilities::registers::register_bitfields;
use kernel::utilities::registers::{register_structs, ReadWrite};
use kernel::utilities::StaticRef;
use kernel::ErrorCode;

pub const TIMG0_BASE: StaticRef<TimgRegisters> =
    unsafe { StaticRef::new(0x6001_F000 as *const TimgRegisters) };

pub const TIMG1_BASE: StaticRef<TimgRegisters> =
    unsafe { StaticRef::new(0x6002_0000 as *const TimgRegisters) };

/// 20MHz `Frequency`
#[derive(Debug)]
pub struct Freq20MHz;
impl time::Frequency for Freq20MHz {
    fn frequency() -> u32 {
        20_000_000
    }
}

register_structs! {
    pub TimgRegisters {
        (0x000 => t0config: ReadWrite<u32, CONFIG::Register>),
        (0x004 => t0lo: ReadWrite<u32>),
        (0x008 => t0hi: ReadWrite<u32>),
        (0x00C => t0update: ReadWrite<u32>),
        (0x010 => t0alarmlo: ReadWrite<u32>),
        (0x014 => t0alarmhi: ReadWrite<u32>),
        (0x018 => t0loadlo: ReadWrite<u32>),
        (0x01C => t0loadhi: ReadWrite<u32>),
        (0x020 => t0load: ReadWrite<u32>),

        (0x024 => _reserved0),

        (0x048 => wdtconfig0: ReadWrite<u32, WDTCONFIG0::Register>),
        (0x04C => wdtconfig1: ReadWrite<u32, WDTCONFIG1::Register>),
        (0x050 => wdtconfig2: ReadWrite<u32>),
        (0x054 => wdtconfig3: ReadWrite<u32>),
        (0x058 => wdtconfig4: ReadWrite<u32>),
        (0x05C => wdtconfig5: ReadWrite<u32>),
        (0x060 => wdtfeed: ReadWrite<u32>),
        (0x064 => wdtwprotect: ReadWrite<u32>),

        (0x068 => rtccalicfg: ReadWrite<u32, RTCCALICFG::Register>),
        (0x06C => rtccalicfg1: ReadWrite<u32, RTCCALICFG1::Register>),

        (0x070 => int_ena: ReadWrite<u32, INT::Register>),
        (0x074 => int_raw: ReadWrite<u32, INT::Register>),
        (0x078 => int_st: ReadWrite<u32, INT::Register>),
        (0x07C => int_clr: ReadWrite<u32, INT::Register>),
        (0x080 => @END),
    }
}

register_bitfields![u32,
    CONFIG [
        USE_XTAL OFFSET(9) NUMBITS(1) [],
        ALARM_EN OFFSET(10) NUMBITS(1) [],
        LEVEL_INT_EN OFFSET(11) NUMBITS(1) [],
        DIVIDER_RST OFFSET(12) NUMBITS(1) [],
        DIVIDER OFFSET(13) NUMBITS(16) [],
        AUTORELOAD OFFSET(29) NUMBITS(1) [],
        INCREASE OFFSET(30) NUMBITS(1) [],
        EN OFFSET(31) NUMBITS(1) [],
    ],
    WDTCONFIG0 [
        FLASHBOOT_MOD_EN OFFSET(14) NUMBITS(1) [],
        SYS_RESET_LENGTH OFFSET(15) NUMBITS(3) [],
        CPU_RESET_LENGTH OFFSET(18) NUMBITS(3) [],
        LEVEL_INT_EN OFFSET(21) NUMBITS(1) [],
        EDGE_INT_EN OFFSET(22) NUMBITS(1) [],
        STG3 OFFSET(23) NUMBITS(2) [],
        STG2 OFFSET(25) NUMBITS(2) [],
        STG1 OFFSET(27) NUMBITS(2) [],
        STG0 OFFSET(29) NUMBITS(2) [],
        EN OFFSET(31) NUMBITS(1) [],
    ],
    WDTCONFIG1 [
        CLK_PRESCALE OFFSET(16) NUMBITS(16) [],
    ],
    RTCCALICFG [
        START_CYCLING OFFSET(12) NUMBITS(1) [],
        CLK_SEL OFFSET(13) NUMBITS(2) [],
        RDY OFFSET(15) NUMBITS(1) [],
        MAX OFFSET(16) NUMBITS(15) [],
        START OFFSET(31) NUMBITS(1) [],
    ],
    RTCCALICFG1 [
        VALUE OFFSET(7) NUMBITS(25) [],
    ],
    INT [
        T0 OFFSET(0) NUMBITS(1) [],
        WDT OFFSET(1) NUMBITS(1) [],
    ],
];

#[derive(Copy, Clone)]
pub enum ClockSource {
    Pll = 0,
    Xtal = 1,
}

pub struct TimG<'a> {
    registers: StaticRef<TimgRegisters>,
    clocksource: ClockSource,
    alarm_client: OptionalCell<&'a dyn time::AlarmClient>,
}

impl TimG<'_> {
    pub const fn new(base: StaticRef<TimgRegisters>, clocksource: ClockSource) -> Self {
        TimG {
            registers: base,
            clocksource,
            alarm_client: OptionalCell::empty(),
        }
    }

    pub fn handle_interrupt(&self) {
        self.registers.int_clr.modify(INT::T0::SET);
        let _ = self.stop();
        self.alarm_client.map(|client| {
            client.alarm();
        });
    }

    pub fn disable_wdt(&self) {
        self.registers
            .wdtconfig0
            .modify(WDTCONFIG0::EN::CLEAR + WDTCONFIG0::FLASHBOOT_MOD_EN::CLEAR);

        if self.registers.wdtconfig0.is_set(WDTCONFIG0::EN)
            || self
                .registers
                .wdtconfig0
                .is_set(WDTCONFIG0::FLASHBOOT_MOD_EN)
        {
            panic!("Can't disable TIMG WDT");
        }
    }
}

impl time::Time for TimG<'_> {
    type Frequency = Freq20MHz;
    type Ticks = Ticks64;

    fn now(&self) -> Self::Ticks {
        // a write (of any value) to T0UPDATE stores the
        // current counter value to T0LO and T0HI
        self.registers.t0update.set(0xABC);
        Self::Ticks::from(
            self.registers.t0lo.get() as u64 + ((self.registers.t0hi.get() as u64) << 32),
        )
    }
}

impl<'a> Counter<'a> for TimG<'a> {
    fn set_overflow_client(&self, _client: &'a dyn time::OverflowClient) {
        // We have no way to know when this happens
    }

    fn start(&self) -> Result<(), ErrorCode> {
        self.registers.t0config.write(CONFIG::EN::SET);

        Ok(())
    }

    fn stop(&self) -> Result<(), ErrorCode> {
        self.registers.t0config.write(CONFIG::EN::CLEAR);

        Ok(())
    }

    fn reset(&self) -> Result<(), ErrorCode> {
        Err(ErrorCode::FAIL)
    }

    fn is_running(&self) -> bool {
        self.registers.t0config.is_set(CONFIG::EN)
    }
}

impl<'a> Alarm<'a> for TimG<'a> {
    fn set_alarm_client(&self, client: &'a dyn time::AlarmClient) {
        self.alarm_client.set(client);
    }

    fn set_alarm(&self, reference: Self::Ticks, dt: Self::Ticks) {
        let now = self.now();
        let mut expire = reference.wrapping_add(dt);
        if !now.within_range(reference, expire) {
            expire = now;
        }

        self.registers
            .t0config
            .modify(CONFIG::ALARM_EN::CLEAR + CONFIG::EN::CLEAR);

        self.registers.t0config.modify(
            CONFIG::USE_XTAL.val(self.clocksource as u32)
                + CONFIG::INCREASE::SET
                + CONFIG::DIVIDER.val(2 * (2 - self.clocksource as u32)),
        );

        self.registers.t0config.modify(CONFIG::DIVIDER_RST::SET);

        let val = expire.into_u64();
        let high = (val >> 32) as u32;
        let low = (val & 0xffffffff) as u32;

        self.registers.t0alarmlo.set(0xFFFF_FFFF);
        self.registers.t0alarmhi.set(high);
        self.registers.t0alarmlo.set(low);

        self.registers.int_ena.modify(INT::T0::SET);

        self.registers
            .t0config
            .modify(CONFIG::ALARM_EN::SET + CONFIG::EN::SET);
    }

    fn get_alarm(&self) -> Self::Ticks {
        Self::Ticks::from(
            self.registers.t0alarmlo.get() as u64 + ((self.registers.t0alarmhi.get() as u64) << 32),
        )
    }

    fn disarm(&self) -> Result<(), ErrorCode> {
        self.registers.t0config.modify(CONFIG::ALARM_EN::CLEAR);

        Ok(())
    }

    fn is_armed(&self) -> bool {
        self.registers.t0config.is_set(CONFIG::ALARM_EN)
    }

    fn minimum_dt(&self) -> Self::Ticks {
        Self::Ticks::from(1 as u64)
    }
}
