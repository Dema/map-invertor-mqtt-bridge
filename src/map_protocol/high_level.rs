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

#[derive(PartialEq, PartialOrd, Debug, Serialize, Primitive, Default)]
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
        self.low_level_protocol.read_answer()?;

        eeprom[0..self.low_level_protocol.last_read_bytes_index]
            .clone_from_slice(self.low_level_protocol.get_actually_read_slice());

        self.low_level_protocol
            .send_command_clean_buffer(LowLevelCommands::ToRead, 0x100, 0xff)?;
        self.low_level_protocol.read_answer()?;

        eeprom[0x100..(0x100 + self.low_level_protocol.last_read_bytes_index)]
            .clone_from_slice(self.low_level_protocol.get_actually_read_slice());
        Ok(())
    }

    pub fn read_status(&mut self, eeprom: &[u8; 560]) -> Result<MapInfo, MapError> {
        let mut map_info = MapInfo::default();
        // let eeprom = self.read_eeprom()?;
        self.low_level_protocol
            .send_command_clean_buffer(LowLevelCommands::ToRead, 0x527, 0x5F)?;

        let res = self.low_level_protocol.read_answer();
        match res {
            Ok(_) => {
                let buffer = self.low_level_protocol.buffer;
                map_info.flag_eco = buffer[0x5F];
                map_info.relay1 = buffer[0x60] & 0b01;
                map_info.relay2 = buffer[0x60] & 0b10;
                map_info.flag_u_net2 = buffer[1];
                //------------3 phase currents calculation---------------------
                map_info.i_ph1 = (buffer[2] as f32 + ((buffer[3] & 0x7F) as f32) * 256.0) / 10.0;
                map_info.i_ph2 = (buffer[4] as f32 + ((buffer[5] & 0x7F) as f32) * 256.0) / 10.0;
                map_info.i_ph3 = (buffer[6] as f32 + ((buffer[7] & 0x7F) as f32) * 256.0) / 10.0;

                map_info.i_ph1 = if buffer[3] & 0x80 == 0 {
                    map_info.i_ph1
                } else {
                    0.0 - map_info.i_ph1
                };
                map_info.i_ph2 = if buffer[5] & 0x80 == 0 {
                    map_info.i_ph2
                } else {
                    0.0 - map_info.i_ph2
                };
                map_info.i_ph3 = if buffer[7] & 0x80 == 0 {
                    map_info.i_ph3
                } else {
                    0.0 - map_info.i_ph3
                };
                map_info.i_acc_3ph = map_info.i_ph1 + map_info.i_ph2 + map_info.i_ph3;
            }
            Err(_) => {
                map_info.flag_eco = 255;
            }
        }

        self.low_level_protocol
            .send_command_clean_buffer(LowLevelCommands::ToRead, 0x400, 0xFF)?;

        self.low_level_protocol.read_answer()?;

        let buffer = self.low_level_protocol.buffer;

        map_info.mode =
            MapModeExtended::from_i32(buffer[0x400 - 0x3FF] as i32).expect("MapMode is unknown");
        map_info.mode = self.real_mode(
            map_info.mode,
            eeprom[0x16B],
            map_info.flag_eco,
            eeprom[0x13C],
            eeprom[0x13B],
            map_info.u_net,
            // eeprom[0x58C],
        );
        map_info.maps_count = if buffer[0x155] == 0xFF {
            1
        } else {
            buffer[0x155]
        };

        map_info.u_net = buffer[0x422 - 0x3ff] as i32;
        if map_info.u_net > 0 {
            map_info.u_net += 100
        }

        map_info.status_char = self.low_level_protocol.buffer[0x402 - 0x3ff];

        map_info.u_acc = (self.low_level_protocol.buffer[0x405 - 0x3FF] as f32 * 256.0
            + self.low_level_protocol.buffer[0x406 - 0x3FF] as f32)
            / 10.0;

        map_info.i_acc = self.low_level_protocol.buffer[0x408 - 0x3FF] as u32 * 2;

        map_info.p_load = self.low_level_protocol.buffer[0x409 - 0x3FF] as u32 * 100;

        map_info.f_acc_over = self.low_level_protocol.buffer[0x41C - 0x3FF];

        map_info.f_net_over = self.low_level_protocol.buffer[0x41D - 0x3FF];

        map_info.i_net = self.low_level_protocol.buffer[0x423 - 0x3FF];

        map_info.p_net = self.low_level_protocol.buffer[0x424 - 0x3FF] as u32 * 100;

        map_info.tf_net = self.low_level_protocol.buffer[0x425 - 0x3FF];
        // закомментировано в оригинале map_info._TFNET = 6250 / map_info._TFNET;

        map_info.th_f_map = self.low_level_protocol.buffer[0x426 - 0x3FF];
        // закомментировано в оригинале map_info._ThFMAP = 6250 / map_info._ThFMAP;

        map_info.u_ou_t_med = self.low_level_protocol.buffer[0x427 - 0x3FF] as u32;
        if map_info.u_ou_t_med > 0 {
            map_info.u_ou_t_med += 100;
        }

        map_info.tf_net_limit = self.low_level_protocol.buffer[0x428 - 0x3FF];
        // закомментировано в оригинале if (map_info._TFNET_Limit!=0) map_info._TFNET_Limit= 2500 / map_info._TFNET_Limit;

        map_info.u_net_limit = self.low_level_protocol.buffer[0x429 - 0x3FF] as u32;
        map_info.u_net_limit += 100;

        map_info.rs_err_sis = self.low_level_protocol.buffer[0x42A - 0x3FF];
        map_info.rs_err_job_m = self.low_level_protocol.buffer[0x42B - 0x3FF];

        map_info.rs_err_job = self.low_level_protocol.buffer[0x42C - 0x3FF];

        map_info.rs_warning = self.low_level_protocol.buffer[0x2E];

        map_info.temp_grad0 = self.low_level_protocol.buffer[0x2F] as i8 - 50;
        map_info.temp_grad1 = self.low_level_protocol.buffer[0x30] as i8 - 50;

        map_info.temp_grad2 = self.low_level_protocol.buffer[0x430 - 0x3FF] as i8 - 50;

        if map_info.i_net < 16 {
            map_info.i_net_16_4 = self.low_level_protocol.buffer[0x32] as f32 / 16.0;
        } else {
            map_info.i_net_16_4 = self.low_level_protocol.buffer[0x32] as f32 / 4.0;
        }

        map_info.i_acc_med_a_u16 = self.low_level_protocol.buffer[0x34] as f32 * 16.0
            + self.low_level_protocol.buffer[0x33] as f32 / 16.0;

        map_info.temp_off = self.low_level_protocol.buffer[0x43C - 0x3FF];
        map_info.e_net = self.low_level_protocol.buffer[0x50] as u32 * 65536
            + self.low_level_protocol.buffer[0x4F] as u32 * 256
            + self.low_level_protocol.buffer[0x4E] as u32;
        map_info.e_acc = self.low_level_protocol.buffer[0x53] as u32 * 65536
            + self.low_level_protocol.buffer[0x52] as u32 * 256
            + self.low_level_protocol.buffer[0x51] as u32;
        map_info.e_acc_charge = self.low_level_protocol.buffer[0x56] as u32 * 65536
            + self.low_level_protocol.buffer[0x55] as u32 * 256
            + self.low_level_protocol.buffer[0x54] as u32;
        map_info.i2_c_err = self.low_level_protocol.buffer[0x45A - 0x3FF];
        map_info.rs_err_dop = self.low_level_protocol.buffer[0x447 - 0x3FF];

        // //---------------------------Checking EEPROM change-------------------------

        // if (self.low_level_protocol.buffer[0x04] & 5 > 0) {
        //     if (self.read_eeprom_to_buffer(eeprom, fd, mysql) == 0) {
        //         self.low_level_protocol.buffer[0] = 3;
        //         send_command(to_write, fd, 0x0, 0x0);
        //         if (read_answer(fd) == 0) {
        //             self.low_level_protocol.buffer[0] = 0;
        //             send_command(to_write, fd, 0x403, 0x0);
        //             read_answer(fd);
        //         }
        //     }
        // }

        Ok(map_info)
    }

    fn real_mode(
        &self,
        mode: MapModeExtended,
        net_alg: u8,
        flag_eco: u8,
        net_up_eco: u8,
        net_up_load: u8,
        unet: i32,
        // Pmax_On: u8,
    ) -> MapModeExtended {
        if mode != MapModeExtended::PowerOnGeneratingNoExternalPower
            || mode != MapModeExtended::PowerOnTranslatingExternalPower
        {
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
