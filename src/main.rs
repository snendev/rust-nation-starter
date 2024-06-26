mod cheats;
use std::time::Duration;

use cheats::internal::infer;

use hs_hackathon::prelude::*;

use cheats::angles::Vector;
use cheats::approaching::Hint;
use cheats::positioning::Position;
use cheats::TeamColors;

const CAR: Color = Color::Blue;
const TARGET: Color = Color::Green;

const TEAM_COLORS: TeamColors = TeamColors {
    car: CAR,
    target: TARGET,
};

#[allow(unused)]
struct MapState {
    car: Position,
    target: Position,
}

#[allow(unused)]
impl MapState {
    pub async fn infer(drone: &mut Camera) -> eyre::Result<Self> {
        infer(&TEAM_COLORS, drone)
            .await
            .map(|(car_bbox, target_bbox)| Self {
                car: car_bbox.into(),
                target: target_bbox.into(),
            })
    }

    async fn car_orientation(
        previous_pos: &MapState,
        current: &MapState,
    ) -> eyre::Result<Vector> {
        Ok(Vector::from((current.car, previous_pos.car)))
    }
}

#[derive(Debug)]
#[allow(unused)]
enum State {
    /// Turn the cars direction by doing consecutive front and back movements
    /// until the angle between the cars orientation and the target converges to be under
    /// a specified threshold
    Turning,
    /// Approach the car by doing incremental actions of approaching and measuring interleaved.
    /// So we approach the target a bit, measure if we decreased the distance, if yes repeat, if no
    /// then calibrate. We do this until we hit the target.
    Approaching,
    /// Simply idling on the target and identifying when the target moves away from our current
    /// position.
    Idle,
}

enum Direction {
    Left,
    Right,
    Straight,
}

impl State {
    async fn execute(
        &mut self,
        drone: &mut Camera,
        motor: &mut MotorSocket,
        wheels: &mut WheelOrientation,
    ) -> eyre::Result<()> {
        match self {
            State::Turning => {
                // get the start position
                // move backward a little
                // get the next position
                let map_state_before = MapState::infer(drone).await?;
                motor.move_for(Velocity::backward(), Duration::from_secs_f32(1.)).await?;
                let map_state_after = MapState::infer(drone).await?;

                // orientation represents the "forward" vector for the car
                let orientation =
                    MapState::car_orientation(&map_state_before, &map_state_after).await?;
                // this must be compared to the vector between car and target
                let car_to_target = Vector::from((map_state_after.car, map_state_after.target));
                let orientation_correction_angle = orientation.angle(car_to_target);

                let angle = if orientation_correction_angle.abs() > 0.01 {
                    Angle::try_from(orientation_correction_angle as f32 / 5.)?
                } else {
                    *self = Self::Approaching;
                    return Ok(());
                };
                wheels.set(angle).await?;
                motor.move_for(Velocity::forward(), Duration::from_secs_f32(1.)).await?;
                wheels.set(Angle::straight()).await?;
            }
            State::Approaching => {
                let hint = cheats::approaching::auto(
                    &TeamColors {
                        car: CAR,
                        target: TARGET,
                    },
                    drone,
                    motor,
                    wheels,
                )
                .await?;

                *self = match hint {
                    Hint::TargetWasHit => Self::Idle,
                    Hint::OrientationIsOff => Self::Turning,
                };
            }
            State::Idle => {
                cheats::idling::auto(
                    &TeamColors {
                        car: CAR,
                        target: TARGET,
                    },
                    drone,
                    motor,
                    wheels,
                )
                .await?;

                *self = Self::Turning;
            }
        }

        Ok(())
    }
}

#[hs_hackathon::main]
async fn main() -> eyre::Result<()> {
    let mut wheels = WheelOrientation::new().await?;
    let mut motor = MotorSocket::open().await?;
    let mut drone = Camera::connect().await?;

    let mut machine = State::Turning;

    loop {
        machine.execute(&mut drone, &mut motor, &mut wheels).await?;
        tracing::debug!("{:?}", machine);
    }
}
