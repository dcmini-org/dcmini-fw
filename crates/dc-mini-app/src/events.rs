use crate::tasks::ads::events::AdsEvent;
use crate::tasks::session::events::SessionEvent;
use crate::{prelude::*, todo};
use derive_more::From;

#[derive(Debug, From)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ButtonPress {
    Single,
    Double,
    Hold,
}

#[derive(Debug, From)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Event {
    AdsEvent(AdsEvent),
    SessionEvent(SessionEvent),
    ButtonPress(ButtonPress),
    TimerElapsed,
    ImuEvent(ImuEvent),
    PowerEvent(PowerEvent),
}

#[embassy_executor::task]
pub async fn orchestrate(
    receiver: EventReceiver,
    ads_manager: AdsManager,
    mut session_manager: SessionManager,
    imu_manager: ImuManager,
    mut power_manager: PowerManager,
) {
    power_manager.handle_event(PowerEvent::Enable).await;

    loop {
        match receiver.receive().await {
            Event::AdsEvent(e) => ads_manager.handle_event(e).await,
            Event::SessionEvent(e) => session_manager.handle_event(e).await,
            Event::ButtonPress(e) => match e {
                ButtonPress::Single => {} // Do nothing
                ButtonPress::Double => {
                    ads_manager.handle_event(AdsEvent::ManualRecord).await;
                }
                ButtonPress::Hold => {
                    info!("Powering down");
                    unwrap!(NEOPIX_CHAN.try_send(NeopixEvent::PowerOff));
                    // TODO: implement SR6 power-off
                }
            },
            Event::TimerElapsed => todo!(),
            Event::ImuEvent(e) => imu_manager.handle_event(e).await,
            Event::PowerEvent(e) => {
                power_manager.handle_event(e).await;
            }
        }
    }
}
