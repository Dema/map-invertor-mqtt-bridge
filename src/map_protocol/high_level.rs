use enum_primitive_derive::Primitive;
use num_traits::FromPrimitive;
use serde::Serialize;
use serialport::SerialPort;

use tracing::instrument;

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

#[derive(Default, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(non_snake_case)]
pub struct MapInfo {
    _MODE: MapModeExtended,
    _Status_Char: u8,
    _Uacc: f32,
    _Iacc: u32,
    _PLoad: u32,
    _F_Acc_Over: u8,
    _F_Net_Over: u8,
    _UNET: i32,
    _INET: u8,
    _PNET: u32,
    _TFNET: u8,
    _ThFMAP: u8,
    _UOUTmed: u32,
    _TFNET_Limit: u8,
    _UNET_Limit: u32,
    _RSErrSis: u8,
    _RSErrJobM: u8,
    _RSErrJob: u8,
    _RSWarning: u8,
    _Temp_Grad0: i8,
    _Temp_Grad2: i8,
    _INET_16_4: f32,
    _IAcc_med_A_u16: f32,
    _Temp_off: u8,
    _E_NET: u32,
    _E_ACC: u32,
    _E_ACC_CHARGE: u32,
    _Uacc_optim: f32,
    _I_acc_avg: f32,
    _I_mppt_avg: f32,
    _I2C_err: u8,
    _Temp_Grad1: i8,
    _Relay1: u8,
    _Relay2: u8,
    _Flag_ECO: u8,
    _RSErrDop: u8,
    _flagUnet2: u8,
    I_ph1: f32,
    I_ph2: f32,
    I_ph3: f32,
    I_acc_3ph: f32,
    MAPS_count: u8,
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
    #[instrument(skip(self))]
    pub fn read_eeprom(&mut self) -> Result<[u8; 560], MapError> {
        let mut eeprom = [0u8; 560];

        self.read_eeprom_to_buffer(&mut eeprom)?;
        Ok(eeprom)
    }
    #[instrument(skip(eeprom, self))]
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

    #[instrument(skip(self))]
    pub fn read_status(&mut self, eeprom: &[u8; 560]) -> Result<MapInfo, MapError> {
        let mut map_info = MapInfo::default();
        // let eeprom = self.read_eeprom()?;
        self.low_level_protocol
            .send_command_clean_buffer(LowLevelCommands::ToRead, 0x527, 0x5F)?;

        let res = self.low_level_protocol.read_answer();
        match res {
            Ok(_) => {
                let buffer = self.low_level_protocol.buffer;
                map_info._Flag_ECO = buffer[0x5F];
                map_info._Relay1 = buffer[0x60] & 1;
                map_info._Relay2 = buffer[0x60] & 2;
                map_info._flagUnet2 = buffer[1];
                //------------3 phase currents calculation---------------------
                map_info.I_ph1 = (buffer[2] as f32 + ((buffer[3] & 0x7F) as f32) * 256.0) / 10.0;
                map_info.I_ph2 = (buffer[4] as f32 + ((buffer[5] & 0x7F) as f32) * 256.0) / 10.0;
                map_info.I_ph3 = (buffer[6] as f32 + ((buffer[7] & 0x7F) as f32) * 256.0) / 10.0;

                map_info.I_ph1 = if buffer[3] & 0x80 == 0 {
                    map_info.I_ph1
                } else {
                    0.0 - map_info.I_ph1
                };
                map_info.I_ph2 = if buffer[5] & 0x80 == 0 {
                    map_info.I_ph2
                } else {
                    0.0 - map_info.I_ph2
                };
                map_info.I_ph3 = if buffer[7] & 0x80 == 0 {
                    map_info.I_ph3
                } else {
                    0.0 - map_info.I_ph3
                };
                map_info.I_acc_3ph = map_info.I_ph1 + map_info.I_ph2 + map_info.I_ph3;
            }
            Err(_) => {
                map_info._Flag_ECO = 255;
            }
        }

        self.low_level_protocol
            .send_command_clean_buffer(LowLevelCommands::ToRead, 0x400, 0xFF)?;

        self.low_level_protocol.read_answer()?;

        let buffer = self.low_level_protocol.buffer;

        map_info._MODE =
            MapModeExtended::from_i32(buffer[0x400 - 0x3FF] as i32).expect("MapMode is unknown");
        map_info._MODE = self.real_mode(
            map_info._MODE,
            eeprom[0x16B],
            map_info._Flag_ECO,
            eeprom[0x13C],
            eeprom[0x13B],
            map_info._UNET,
            // eeprom[0x58C],
        );
        map_info.MAPS_count = if buffer[0x155] == 0xFF {
            1
        } else {
            buffer[0x155]
        };

        map_info._UNET = buffer[0x422 - 0x3ff] as i32;
        if map_info._UNET > 0 {
            map_info._UNET += 100
        }

        map_info._Status_Char = self.low_level_protocol.buffer[0x402 - 0x3ff];

        map_info._Uacc = (self.low_level_protocol.buffer[0x405 - 0x3FF] as f32 * 256.0
            + self.low_level_protocol.buffer[0x406 - 0x3FF] as f32)
            / 10.0;

        map_info._Iacc = self.low_level_protocol.buffer[0x408 - 0x3FF] as u32 * 2;

        map_info._PLoad = self.low_level_protocol.buffer[0x409 - 0x3FF] as u32 * 100;

        map_info._F_Acc_Over = self.low_level_protocol.buffer[0x41C - 0x3FF];

        map_info._F_Net_Over = self.low_level_protocol.buffer[0x41D - 0x3FF];

        map_info._INET = self.low_level_protocol.buffer[0x423 - 0x3FF];

        map_info._PNET = self.low_level_protocol.buffer[0x424 - 0x3FF] as u32 * 100;

        map_info._TFNET = self.low_level_protocol.buffer[0x425 - 0x3FF];
        // закомментировано в оригинале map_info._TFNET = 6250 / map_info._TFNET;

        map_info._ThFMAP = self.low_level_protocol.buffer[0x426 - 0x3FF];
        // закомментировано в оригинале map_info._ThFMAP = 6250 / map_info._ThFMAP;

        map_info._UOUTmed = self.low_level_protocol.buffer[0x427 - 0x3FF] as u32;
        if map_info._UOUTmed > 0 {
            map_info._UOUTmed += 100;
        }

        map_info._TFNET_Limit = self.low_level_protocol.buffer[0x428 - 0x3FF];
        // закомментировано в оригинале if (map_info._TFNET_Limit!=0) map_info._TFNET_Limit= 2500 / map_info._TFNET_Limit;

        map_info._UNET_Limit = self.low_level_protocol.buffer[0x429 - 0x3FF] as u32;
        map_info._UNET_Limit += 100;

        map_info._RSErrSis = self.low_level_protocol.buffer[0x42A - 0x3FF];
        map_info._RSErrJobM = self.low_level_protocol.buffer[0x42B - 0x3FF];

        map_info._RSErrJob = self.low_level_protocol.buffer[0x42C - 0x3FF];

        map_info._RSWarning = self.low_level_protocol.buffer[0x2E];

        map_info._Temp_Grad0 = self.low_level_protocol.buffer[0x2F] as i8 - 50;
        map_info._Temp_Grad1 = self.low_level_protocol.buffer[0x30] as i8 - 50;

        map_info._Temp_Grad2 = self.low_level_protocol.buffer[0x430 - 0x3FF] as i8 - 50;

        if map_info._INET < 16 {
            map_info._INET_16_4 = self.low_level_protocol.buffer[0x32] as f32 / 16.0;
        } else {
            map_info._INET_16_4 = self.low_level_protocol.buffer[0x32] as f32 / 4.0;
        }

        map_info._IAcc_med_A_u16 = self.low_level_protocol.buffer[0x34] as f32 * 16.0
            + self.low_level_protocol.buffer[0x33] as f32 / 16.0;

        map_info._Temp_off = self.low_level_protocol.buffer[0x43C - 0x3FF];
        map_info._E_NET = self.low_level_protocol.buffer[0x50] as u32 * 65536
            + self.low_level_protocol.buffer[0x4F] as u32 * 256
            + self.low_level_protocol.buffer[0x4E] as u32;
        map_info._E_ACC = self.low_level_protocol.buffer[0x53] as u32 * 65536
            + self.low_level_protocol.buffer[0x52] as u32 * 256
            + self.low_level_protocol.buffer[0x51] as u32;
        map_info._E_ACC_CHARGE = self.low_level_protocol.buffer[0x56] as u32 * 65536
            + self.low_level_protocol.buffer[0x55] as u32 * 256
            + self.low_level_protocol.buffer[0x54] as u32;
        map_info._I2C_err = self.low_level_protocol.buffer[0x45A - 0x3FF];
        map_info._RSErrDop = self.low_level_protocol.buffer[0x447 - 0x3FF];

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

    #[instrument]
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
                    // return match (flag_eco & 1).cmp(&0u8) {
                    //     Ordering::Greater => MapModeExtended::WaitingForExternalCharge,
                    //     Ordering::Equal => MapModeExtended::TranslationECOPumping,
                    //     Ordering::Less => mode,
                    // };
                    if flag_eco & 1 > 0 {
                        return MapModeExtended::WaitingForExternalCharge;
                    } else if flag_eco & 1 == 0 {
                        return MapModeExtended::TranslationECOPumping;
                    } else {
                        return mode;
                    }
                } else if net_alg == 3 {
                    // return match (flag_eco & 2).cmp(&0u8) {
                    //     Ordering::Greater => MapModeExtended::SellingBackToGridMinRate,
                    //     Ordering::Equal => MapModeExtended::SellingBackToGridTranslationEcoPumping,
                    //     Ordering::Less => mode,
                    // };
                    if flag_eco & 2 > 0 {
                        return MapModeExtended::SellingBackToGridMinRate;
                    } else if flag_eco & 2 == 0 {
                        return MapModeExtended::SellingBackToGridTranslationEcoPumping;
                    } else {
                        return mode;
                    }
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
