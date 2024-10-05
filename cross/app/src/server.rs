use crc::{Crc, CRC_32_ISCSI};
use dimmer_communication::{ChannelDuty, ChannelEnabled, CHANNELS_COUNT, ClientCommand, ClientCommandResult, ClientCommandResultType, ClientCommandType, GroupFrequency, PINS_COUNT, PinState};
use postcard::{from_bytes_crc32, to_slice_crc32};
use crate::communication::{BufferWriter, LedState};
use crate::errors::Error;
use crate::pwm_service::PwmChannels;
use crate::storage::Storage;

const MAX_PROCESSED_IDS: usize = 50;

pub struct Server {
    processed_ids: [u32; MAX_PROCESSED_IDS],
    processed_count: u8,
    processed_overflow: bool,
    //buff: [u8; 256],
    crc: Crc::<u32>,
}

impl Server {
    pub fn new() -> Self {
        Server {
            processed_ids: [0; MAX_PROCESSED_IDS],
            processed_count: 0,
            processed_overflow: false,
            //buff: [0u8; 256],
            crc: Crc::<u32>::new(&CRC_32_ISCSI),
        }
    }

    pub fn idle(
        &mut self, 
        input_data: &[u8],
        out_buffer: &mut dyn BufferWriter, 
        storage: &mut Storage, 
        channels: &mut PwmChannels,
        leds_state: &mut LedState,
    ) -> Result<(), Error> {


        if input_data.len() == 1 {
            let value = input_data[0];
            if value < 8 {
                let on = (value & 0x04) >> 2;
                let led = value & 0x03;
                leds_state.set_high(led, on == 1);
            } else if value > 15 && value < 32 {
                leds_state.set_mask(value & 0x0F);
            }
        }

        // out_buffer.add_str("value: ")?;
        // out_buffer.add(input_data)?;

        let command: ClientCommand = from_bytes_crc32(input_data, self.crc.digest())
            .map_err(|err| { Error::SerializedDataError(err) })?;
        // to_slice_crc32(&ClientCommandType::SetChannelDuty(ChannelDuty::new(1, 0.7)), &mut buff, crc.digest()).map(|b| {
        //     out_buffer.add(b)
        // }).map_err(|_| {Error::SerializedDataError})?;

        match command.data {
            ClientCommandType::SetChannelDuty(data) => {
                match channels.set_channel_duty(data.channel, data.duty) {
                    Ok(duty) => {
                        send_answer(&ClientCommandResult::channel_duty(command.id,data.channel, duty), out_buffer)
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::SetChannelEnabled ( ChannelEnabled { channel, enabled } ) => {
                match channels.set_enabled_channel(channel, enabled) {
                    Ok(_) => {
                        send_answer(&ClientCommandResult::channel_enabled(command.id,channel, enabled), out_buffer)
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::SetAllChannelsSameEnabled { enabled } => {
                channels.set_enabled_all(enabled);
                send_answer(&ClientCommandResult::all_channels_enabled(command.id, enabled), out_buffer)
            },
            ClientCommandType::SetAllChannelsSameDuty(duty) => {
                channels.set_all_duty(duty);
                send_answer(&ClientCommandResult::all_channels_same_duty(command.id, duty), out_buffer)
            },
            ClientCommandType::GetChannelDuty { channel } => {
                match channels.get_channel_duty(channel) {
                    Ok(duty) => {
                        send_answer(&ClientCommandResult::channel_duty(command.id, channel, duty), out_buffer)
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::GetChannelCount => {
                let count = channels.get_channels_count();
                send_answer(&ClientCommandResult::channel_count(command.id, count), out_buffer)
            },
            ClientCommandType::GetAllChannelsDuty => {
                let mut data: [ChannelDuty; CHANNELS_COUNT] =
                    [ChannelDuty::new(0, 0.0); CHANNELS_COUNT];
                for i in 0..CHANNELS_COUNT {
                    data[i].channel = i as u8;
                    data[i].duty = channels.get_channel_duty(i as u8)?;
                }
                send_answer(&ClientCommandResult::all_channels_duty(command.id, data), out_buffer)
            },
            ClientCommandType::SetGroupFrequency (data) => {
                match storage.read() {
                    Ok(mut pwm_settings) => {
                        pwm_settings.set_group_freq(data.group, data.frequency)?;
                        storage.save(&pwm_settings)
                        //TODO: restart board
                        //no answer needed due to restart
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::GetGroupFrequency(group) => {
                match storage.read() {
                    Ok(pwm_settings) => {
                        let freq = pwm_settings.get_group_freq(group)?;
                        send_answer(&ClientCommandResult::group_frequency(command.id, group, freq), out_buffer)
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::SetAllGroupsSameFrequency (freq) => {
                match storage.read() {
                    Ok(mut pwm_settings) => {
                        pwm_settings.set_all_groups_same_freq(freq);
                        match storage.save(&pwm_settings) {
                            Ok(_) => {
                                //TODO: restart board
                                //no answer needed due to restart
                                Ok(())
                            },
                            Err(e) => {
                                return send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                            }
                        }
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::SetAllGroupsFrequency (data) => {
                match storage.read() {
                    Ok(mut pwm_settings) => {
                        pwm_settings.set_all_groups_freq(data);
                        match storage.save(&pwm_settings) {
                            Ok(_) => {
                                //TODO: restart board
                                //no answer needed due to restart
                                Ok(())
                            }
                            Err(e) => {
                                return send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                            }
                        }
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::GetAllGroupsFrequency => {
                match storage.read() {
                    Ok(pwm_settings) => {
                        let freq = pwm_settings.get_all_groups_freq();
                        send_answer(&ClientCommandResult::all_groups_frequency(command.id, freq), out_buffer)
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::GetDateTimestamp => {
                //TODO: implement
                Ok(())
            },
            ClientCommandType::SetDateTimestamp { timestamp: u64 } => {
                //TODO: implement
                Ok(())
            },
            ClientCommandType::SetPinState (PinState { pin, state }) => {
                leds_state.set_high(pin, state);
                let state = leds_state.get_high(pin);
                send_answer(&ClientCommandResult::pin_state(command.id, pin, state), out_buffer)
            },
            ClientCommandType::GetPinState (pin) => {
                let state = leds_state.get_high(pin);
                send_answer(&ClientCommandResult::pin_state(command.id, pin, state), out_buffer)
            },
            ClientCommandType::GetAllPinsState => {
                let mut data: [bool; PINS_COUNT] = [false; 8];
                for i in 0..8 {
                    data[i] = leds_state.get_high(i);
                }
                send_answer(&ClientCommandResult::all_pins_state(command.id, data), out_buffer)
            },
        }
    }
}

fn send_answer(value: &ClientCommandResult, out_buffer: &mut dyn BufferWriter) -> Result<(), Error> {
    let mut buff = [0u8; 256];
    let crc = Crc::<u32>::new(&CRC_32_ISCSI);
    match to_slice_crc32(value, &mut buff, crc.digest()) {
        Ok(b) => {
            out_buffer.add(b)
        },
        Err(e) => {
            Err(Error::SerializedDataError(e))
        }
    }
}