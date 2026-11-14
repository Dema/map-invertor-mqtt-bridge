use std::cmp::Ordering;

use enum_primitive_derive::Primitive;
use num_traits::FromPrimitive;
use serde::Serialize;
use serialport::SerialPort;

use super::{
    low_level::{LowLevelCommands, LowLevelProtocol},
    MapError,
};

// pub struct BMSThreshold;

// impl BMSThreshold {
//     pub const BMS_HIGH_U: f64 = 3.9;
//     pub const BMS_LOW_U: f64 = 2.7;
//     pub const BMS_HIGH_T: u8 = 40;
//     pub const BMS_LOW_T: u8 = 0;
// }

// Inverter protocol constants
pub struct InverterConstants;

impl InverterConstants {
    // Register addresses
    pub const MODE_REGISTER_ADDRESS: usize = 0x400;
    /// First register of the status read, so the payload of the `0x400` read is
    /// indexed as `register address - REGISTER_BASE_ADDRESS`.
    pub const REGISTER_BASE_ADDRESS: usize = 0x400;
    pub const U_NET_REGISTER_OFFSET: usize = 0x422;
    /// Offset inside the status answer, not a register address: mapd.c read it
    /// from `eeprom[0x155]`, this port reads it from the status answer.
    pub const MAPS_COUNT_REGISTER_OFFSET: usize = 0x154;
    pub const FLAG_ECO_REGISTER_OFFSET: usize = 0x5F;
    
    // Status and control values
    pub const MAX_BYTE_VALUE: u8 = 0xFF;
    pub const FLAG_ECO_ERROR_VALUE: u8 = 255;
    
    // Calculation factors
    pub const VOLTAGE_OFFSET: i32 = 100;
    pub const CURRENT_DIVISOR: f32 = 10.0;
    pub const BYTE_MULTIPLIER: f32 = 256.0;
    pub const I_ACC_MULTIPLIER: u32 = 2;
    pub const POWER_MULTIPLIER: u32 = 100;
    pub const HIGH_BYTE_MULTIPLIER: f32 = 65536.0;
    pub const TEMPERATURE_OFFSET: i8 = 50;
    pub const LOW_CURRENT_DIVISOR: f32 = 16.0;
    pub const HIGH_CURRENT_DIVISOR: f32 = 4.0;
}

#[derive(Default, Debug, Serialize, PartialEq)]
pub struct MapInfo {
    mode: MapModeExtended,
    status_char: u8,
    u_acc: f32,
    i_acc: u32,
    p_load: u32,
    f_acc_over: u8,
    f_net_over: u8,
    u_net: i32,
    i_net: u8,
    p_net: u32,
    tf_net: u8,
    th_f_map: u8,
    u_ou_t_med: u32,
    tf_net_limit: u8,
    u_net_limit: u32,
    rs_err_sis: u8,
    rs_err_job_m: u8,
    rs_err_job: u8,
    rs_warning: u8,
    temp_grad0: i8,
    temp_grad1: i8,
    temp_grad2: i8,
    i_net_16_4: f32,
    i_acc_med_a_u16: f32,
    temp_off: u8,
    e_net: u32,
    e_acc: u32,
    e_acc_charge: u32,
    u_acc_optim: f32,
    i_acc_avg: f32,
    i_mppt_avg: f32,
    i2_c_err: u8,
    relay1: u8,
    relay2: u8,
    flag_eco: u8,
    rs_err_dop: u8,
    flag_u_net2: u8,
    i_ph1: f32,
    i_ph2: f32,
    i_ph3: f32,
    i_acc_3ph: f32,
    maps_count: u8,
}

impl MapInfo {
    pub fn mode(&self) -> MapModeExtended {
        self.mode
    }
    pub fn status_char(&self) -> u8 {
        self.status_char
    }
    pub fn u_acc(&self) -> f32 {
        self.u_acc
    }
    pub fn i_acc(&self) -> u32 {
        self.i_acc
    }
    pub fn p_load(&self) -> u32 {
        self.p_load
    }
    pub fn f_acc_over(&self) -> u8 {
        self.f_acc_over
    }
    pub fn f_net_over(&self) -> u8 {
        self.f_net_over
    }
    pub fn u_net(&self) -> i32 {
        self.u_net
    }
    pub fn i_net(&self) -> u8 {
        self.i_net
    }
    pub fn p_net(&self) -> u32 {
        self.p_net
    }
    pub fn tf_net(&self) -> u8 {
        self.tf_net
    }
    pub fn th_f_map(&self) -> u8 {
        self.th_f_map
    }
    pub fn u_ou_t_med(&self) -> u32 {
        self.u_ou_t_med
    }
    pub fn tf_net_limit(&self) -> u8 {
        self.tf_net_limit
    }
    pub fn u_net_limit(&self) -> u32 {
        self.u_net_limit
    }
    pub fn rs_err_sis(&self) -> u8 {
        self.rs_err_sis
    }
    pub fn rs_err_job_m(&self) -> u8 {
        self.rs_err_job_m
    }
    pub fn rs_err_job(&self) -> u8 {
        self.rs_err_job
    }
    pub fn rs_warning(&self) -> u8 {
        self.rs_warning
    }
    pub fn temp_grad0(&self) -> i8 {
        self.temp_grad0
    }
    pub fn temp_grad1(&self) -> i8 {
        self.temp_grad1
    }
    pub fn temp_grad2(&self) -> i8 {
        self.temp_grad2
    }
    pub fn i_net_16_4(&self) -> f32 {
        self.i_net_16_4
    }
    pub fn i_acc_med_a_u16(&self) -> f32 {
        self.i_acc_med_a_u16
    }
    pub fn temp_off(&self) -> u8 {
        self.temp_off
    }
    pub fn e_net(&self) -> u32 {
        self.e_net
    }
    pub fn e_acc(&self) -> u32 {
        self.e_acc
    }
    pub fn e_acc_charge(&self) -> u32 {
        self.e_acc_charge
    }
    pub fn u_acc_optim(&self) -> f32 {
        self.u_acc_optim
    }
    pub fn i_acc_avg(&self) -> f32 {
        self.i_acc_avg
    }
    pub fn i_mppt_avg(&self) -> f32 {
        self.i_mppt_avg
    }
    pub fn i2_c_err(&self) -> u8 {
        self.i2_c_err
    }
    pub fn relay1(&self) -> u8 {
        self.relay1
    }
    pub fn relay2(&self) -> u8 {
        self.relay2
    }
    pub fn flag_eco(&self) -> u8 {
        self.flag_eco
    }
    pub fn rs_err_dop(&self) -> u8 {
        self.rs_err_dop
    }
    pub fn flag_u_net2(&self) -> u8 {
        self.flag_u_net2
    }
    pub fn i_ph1(&self) -> f32 {
        self.i_ph1
    }
    pub fn i_ph2(&self) -> f32 {
        self.i_ph2
    }
    pub fn i_ph3(&self) -> f32 {
        self.i_ph3
    }
    pub fn i_acc_3ph(&self) -> f32 {
        self.i_acc_3ph
    }
    pub fn maps_count(&self) -> u8 {
        self.maps_count
    }
}

use std::fmt;

impl fmt::Display for MapInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Inverter Status:")?;
        writeln!(f, "  Mode: {:?}", self.mode)?;
        writeln!(f, "  Status Char: {}", self.status_char)?;
        writeln!(f, "  Accumulator Voltage: {:.2} V", self.u_acc)?;
        writeln!(f, "  Accumulator Current: {} A", self.i_acc)?;
        writeln!(f, "  Load Power: {} W", self.p_load)?;
        writeln!(f, "  Network Voltage: {} V", self.u_net)?;
        writeln!(f, "  Network Current: {} A", self.i_net)?;
        writeln!(f, "  Network Power: {} W", self.p_net)?;
        writeln!(f, "  Maps Count: {}", self.maps_count)?;
        Ok(())
    }
}

#[derive(PartialEq, PartialOrd, Debug, Serialize, Primitive, Default, Copy, Clone)]
#[repr(u8)]
pub enum MapModeExtended {
    /// МАП выключен и нет сети на входе
    #[default]
    PowerOff = 0,
    /// МАП выключен но есть сеть на входе (значение напряжения сети выводится в ЖКИ)
    PowerOffExternalPowerPresent = 1,
    /// МАП включен (происходит генерация 220В от АКБ, нет сети на входе.
    PowerOnGeneratingNoExternalPower = 2,
    /// МАП включен и транслирует сеть (есть сеть на входе).
    PowerOnTranslatingExternalPower = 3,
    /// МАП включен, транслирует сеть и одновременно заряжает АКБ.
    PowerOnTranslatingExternalPowerAndCharging = 4,

    // ------------ my extensions------------------------
    /// принудительная генерация
    ForcedGeneration = 10,
    /// тарифная сеть. максимальный тариф. принудительная генерация
    SellingBackToGridMaxRateForcedGeneration = 11,
    /// тарифная сеть. минимальный тариф
    SellingBackToGridMinRate = 12,
    /// трансляция + эко-подкачка
    TranslationECOPumping = 13,
    /// трансляция + продажа в сеть
    TranslationSellingBackToGrid = 14,
    /// ожидание внешнего заряда
    WaitingForExternalCharge = 15,
    /// тарифная сеть. трансляция+эко-подкачка
    SellingBackToGridTranslationEcoPumping = 16,
    /// тарифная сеть. трансляция+продажа в сеть
    SellingBackToGridTranslation = 17,
    /// режим подкачка Pmax
    Pmax = 18,
}

#[derive(Debug)]
pub struct HighLevelProtocol {
    low_level_protocol: LowLevelProtocol,
}
impl HighLevelProtocol {
    pub fn new(port: Box<dyn SerialPort>) -> Result<Self, MapError> {
        Ok(Self {
            low_level_protocol: LowLevelProtocol::new(port),
        })
    }

    pub fn read_eeprom(&mut self) -> Result<[u8; 560], MapError> {
        let mut eeprom = [0u8; 560];

        self.read_eeprom_to_buffer(&mut eeprom)?;
        Ok(eeprom)
    }

    pub fn read_eeprom_to_buffer(&mut self, eeprom: &mut [u8; 560]) -> Result<(), MapError> {
        self.low_level_protocol
            .send_command_clean_buffer(LowLevelCommands::ToRead, 0, 0xFF)?;
        let answer = self.low_level_protocol.exchange()?;
        eeprom[0..answer.data.len()].copy_from_slice(&answer.data);

        self.low_level_protocol
            .send_command_clean_buffer(LowLevelCommands::ToRead, 0x100, 0xff)?;
        let answer = self.low_level_protocol.exchange()?;
        eeprom[0x100..(0x100 + answer.data.len())].copy_from_slice(&answer.data);
        Ok(())
    }

    pub fn read_status(&mut self, eeprom: &[u8; 560]) -> Result<MapInfo, MapError> {
        let mut map_info = MapInfo::default();
        // let eeprom = self.read_eeprom()?;
        self.low_level_protocol
            .send_command_clean_buffer(LowLevelCommands::ToRead, 0x527, 0x5F)?;

        let res = self.low_level_protocol.exchange();
        match res {
            Ok(answer) => {
                let data = answer.data;
                map_info.flag_eco = data[0x5E];
                map_info.relay1 = data[0x5F] & 0b01;
                map_info.relay2 = data[0x5F] & 0b10;
                map_info.flag_u_net2 = data[0];
                //------------3 phase currents calculation---------------------
                map_info.i_ph1 = (data[1] as f32 + ((data[2] & 0x7F) as f32) * 256.0) / 10.0;
                map_info.i_ph2 = (data[3] as f32 + ((data[4] & 0x7F) as f32) * 256.0) / 10.0;
                map_info.i_ph3 = (data[5] as f32 + ((data[6] & 0x7F) as f32) * 256.0) / 10.0;

                map_info.i_ph1 = if data[2] & 0x80 == 0 {
                    map_info.i_ph1
                } else {
                    0.0 - map_info.i_ph1
                };
                map_info.i_ph2 = if data[4] & 0x80 == 0 {
                    map_info.i_ph2
                } else {
                    0.0 - map_info.i_ph2
                };
                map_info.i_ph3 = if data[6] & 0x80 == 0 {
                    map_info.i_ph3
                } else {
                    0.0 - map_info.i_ph3
                };
                map_info.i_acc_3ph = map_info.i_ph1 + map_info.i_ph2 + map_info.i_ph3;
            }
            Err(_) => {
                map_info.flag_eco = InverterConstants::FLAG_ECO_ERROR_VALUE;
            }
        }

        self.low_level_protocol
            .send_command_clean_buffer(LowLevelCommands::ToRead, 0x400, 0xFF)?;

        let answer = self.low_level_protocol.exchange()?;

        // `data` is the payload of the read that starts at register 0x400, with the
        // status byte mapd.c kept at index 0 stripped: a register sits at
        // `address - REGISTER_BASE_ADDRESS`, and the bare offsets below are one less
        // than the raw indices mapd.c used.
        let data = answer.data;

        map_info.mode =
            MapModeExtended::from_i32(data[InverterConstants::MODE_REGISTER_ADDRESS - InverterConstants::REGISTER_BASE_ADDRESS] as i32).expect("MapMode is unknown");
        map_info.mode = self.real_mode(
            map_info.mode,
            eeprom[0x16B],
            map_info.flag_eco,
            eeprom[0x13C],
            eeprom[0x13B],
            map_info.u_net,
            // eeprom[0x58C],
        );
        // mapd.c takes the MAP count from `eeprom[0x155]`; this port reads it from the
        // status answer, where that offset is past the end of the payload. The shared
        // buffer used to be zeroed before a read, so this has always been 0 here.
        let maps_count = data
            .get(InverterConstants::MAPS_COUNT_REGISTER_OFFSET)
            .copied()
            .unwrap_or(0);
        map_info.maps_count = if maps_count == InverterConstants::MAX_BYTE_VALUE {
            1
        } else {
            maps_count
        };

        map_info.u_net = data[InverterConstants::U_NET_REGISTER_OFFSET - InverterConstants::REGISTER_BASE_ADDRESS] as i32;
        if map_info.u_net > 0 {
            map_info.u_net += InverterConstants::VOLTAGE_OFFSET
        }

        map_info.status_char = data[0x402 - InverterConstants::REGISTER_BASE_ADDRESS];

        map_info.u_acc = (data[0x405 - InverterConstants::REGISTER_BASE_ADDRESS] as f32 * InverterConstants::BYTE_MULTIPLIER
            + data[0x406 - InverterConstants::REGISTER_BASE_ADDRESS] as f32)
            / InverterConstants::CURRENT_DIVISOR;

        map_info.i_acc = data[0x408 - InverterConstants::REGISTER_BASE_ADDRESS] as u32 * InverterConstants::I_ACC_MULTIPLIER;

        map_info.p_load = data[0x409 - InverterConstants::REGISTER_BASE_ADDRESS] as u32 * 100;

        map_info.f_acc_over = data[0x41C - InverterConstants::REGISTER_BASE_ADDRESS];

        map_info.f_net_over = data[0x41D - InverterConstants::REGISTER_BASE_ADDRESS];

        map_info.i_net = data[0x423 - InverterConstants::REGISTER_BASE_ADDRESS];

        map_info.p_net = data[0x424 - InverterConstants::REGISTER_BASE_ADDRESS] as u32 * 100;

        map_info.tf_net = data[0x425 - InverterConstants::REGISTER_BASE_ADDRESS];
        // закомментировано в оригинале map_info._TFNET = 6250 / map_info._TFNET;

        map_info.th_f_map = data[0x426 - InverterConstants::REGISTER_BASE_ADDRESS];
        // закомментировано в оригинале map_info._ThFMAP = 6250 / map_info._ThFMAP;

        map_info.u_ou_t_med = data[0x427 - InverterConstants::REGISTER_BASE_ADDRESS] as u32;
        if map_info.u_ou_t_med > 0 {
            map_info.u_ou_t_med += 100;
        }

        map_info.tf_net_limit = data[0x428 - InverterConstants::REGISTER_BASE_ADDRESS];
        // закомментировано в оригинале if (map_info._TFNET_Limit!=0) map_info._TFNET_Limit= 2500 / map_info._TFNET_Limit;

        map_info.u_net_limit = data[0x429 - InverterConstants::REGISTER_BASE_ADDRESS] as u32;
        map_info.u_net_limit += 100;

        map_info.rs_err_sis = data[0x42A - InverterConstants::REGISTER_BASE_ADDRESS];
        map_info.rs_err_job_m = data[0x42B - InverterConstants::REGISTER_BASE_ADDRESS];

        map_info.rs_err_job = data[0x42C - InverterConstants::REGISTER_BASE_ADDRESS];

        map_info.rs_warning = data[0x2D];

        map_info.temp_grad0 = data[0x2E] as i8 - 50;
        map_info.temp_grad1 = data[0x2F] as i8 - 50;

        map_info.temp_grad2 = data[0x430 - InverterConstants::REGISTER_BASE_ADDRESS] as i8 - 50;

        if map_info.i_net < 16 {
            map_info.i_net_16_4 = data[0x31] as f32 / 16.0;
        } else {
            map_info.i_net_16_4 = data[0x31] as f32 / 4.0;
        }

        map_info.i_acc_med_a_u16 = data[0x33] as f32 * 16.0
            + data[0x32] as f32 / 16.0;

        map_info.temp_off = data[0x43C - InverterConstants::REGISTER_BASE_ADDRESS];
        map_info.e_net = data[0x4F] as u32 * 65536
            + data[0x4E] as u32 * 256
            + data[0x4D] as u32;
        map_info.e_acc = data[0x52] as u32 * 65536
            + data[0x51] as u32 * 256
            + data[0x50] as u32;
        map_info.e_acc_charge = data[0x55] as u32 * 65536
            + data[0x54] as u32 * 256
            + data[0x53] as u32;
        map_info.i2_c_err = data[0x45A - InverterConstants::REGISTER_BASE_ADDRESS];
        map_info.rs_err_dop = data[0x447 - InverterConstants::REGISTER_BASE_ADDRESS];

        Ok(map_info)
    }

    fn real_mode(
        &self,
        mode: MapModeExtended,
        net_alg: u8,
        flag_eco: u8,
        net_up_eco: u8,
        _net_up_load: u8,
        unet: i32,
        // Pmax_On: u8,
    ) -> MapModeExtended {
        if matches!(
            mode,
            MapModeExtended::PowerOnGeneratingNoExternalPower
                | MapModeExtended::PowerOnTranslatingExternalPower
        ) {
            // Process these specific modes
        } else {
            return mode;
        }

        if net_up_eco == 0 {
            // ECO forced gen or Tarifs
            // if NetUpLoad == 1 && (Pmax_On & 2 > 0) && (UNET > 100) {
            //     return MapModeExtended::Pmax;
            // }
            if mode == MapModeExtended::PowerOnGeneratingNoExternalPower {
                if unet > 100 {
                    if net_alg == 2 {
                        return MapModeExtended::ForcedGeneration;
                    } else if net_alg == 3 {
                        if (flag_eco & 2) == 0 {
                            return MapModeExtended::SellingBackToGridMaxRateForcedGeneration;
                        } else {
                            return MapModeExtended::SellingBackToGridMinRate;
                        }
                    }
                }
                MapModeExtended::PowerOnGeneratingNoExternalPower
            } else {
                mode
            }
        } else if net_up_eco == 1 {
            // Eco pumping

            if mode == MapModeExtended::PowerOnTranslatingExternalPower {
                if net_alg == 2 {
                    return match (flag_eco & 1).cmp(&0u8) {
                        Ordering::Greater => MapModeExtended::WaitingForExternalCharge,
                        Ordering::Equal => MapModeExtended::TranslationECOPumping,
                        Ordering::Less => mode,
                    };
                } else if net_alg == 3 {
                    return match (flag_eco & 2).cmp(&0u8) {
                        Ordering::Greater => MapModeExtended::SellingBackToGridMinRate,
                        Ordering::Equal => MapModeExtended::SellingBackToGridTranslationEcoPumping,
                        Ordering::Less => mode,
                    };
                } else {
                    return mode;
                }
            } else {
                return mode;
            }
        } else if net_up_eco == 2 {
            // Sell to network
            if mode == MapModeExtended::PowerOnTranslatingExternalPower {
                if net_alg == 2 {
                    if flag_eco & 1 > 0 {
                        return MapModeExtended::WaitingForExternalCharge;
                    } else {
                        return MapModeExtended::TranslationSellingBackToGrid;
                    }
                } else {
                    return mode;
                }
            } else {
                return mode;
            }
        } else {
            return mode;
        }
    }
}

