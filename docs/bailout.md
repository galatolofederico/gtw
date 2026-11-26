# Bailout and parachute

Leave an aircraft in flight, fall, open a parachute, steer it and land on
foot. The empty aircraft keeps flying: it is banked gently away from the
player and its engine goes to idle, and it is stepped by the normal flight
model (and can crash, burn and leave a wreck) while the player is under
canopy.

A moving car can be left the same way: the car carries on driverless while
the body keeps its momentum, rolls along the ground and gets back up.

This is a game model. It is shaped by sport-parachuting figures, not a
certified skydiving simulation.

## Controls

| Where | Input | Does |
| --- | --- | --- |
| In flight | `F7`, or the gamepad's Y alone | Bail out |
| In a moving car | `F7`, `E`, or the gamepad's Y | Bail out; the car carries on driverless |
| Freefall | `W` / `S` | Track forward / back |
| Freefall | `A` / `D` | Turn |
| Freefall | `Space`, the gamepad's A, or the `OPEN PARACHUTE` button | Open the parachute |
| Under canopy | `A` / `D` | Steer |
| Under canopy | `W` | Speed up (more glide, more descent) |
| Under canopy | `S` | Flare (less glide, less descent) |
| In a fall | `C` / `V` | Shoulder / eye camera |
| In a fall | `R` | Put the body on the last safe ground |

Every bailout starts in freefall with the parachute still packed. Nothing
opens it automatically: press the jump control, or click the button on the
fall HUD, when you want the canopy out. A pull below about 100 m may not have
time to inflate: the canopy is only partly open at touchdown and the landing
is hard.

## Exit

- Fighters (F-22, F-4, F/A-18, SR-71) use an ejection seat: the pilot leaves
  upward and clear of the tail, then falls.
- Airliners (747, G650, Concorde) leave through a door.
- Light aircraft jump from an open canopy or cabin.

The seat, door animation and canopy jettison are simplified: the canopy is
opened where the type has one, and the body starts a little above and to the
side of the cockpit. The pilot's body is the same 1.80 m capsule the on-foot
controller uses.

## Leaving a moving car

Above about 4 m/s (14 km/h), `E`, `F7` or the gamepad's Y is a bailout rather
than an ordinary stop-and-get-out. The body leaves on the clear side with the
car's velocity, plus a sideways push and a small upward hop. It then lands,
somersaults forward along the ground for about 1.15 s, then plants a hand and
foot and takes 0.85 s to stand up; the controls are ignored until it is on its
feet.

The car is not braked or stopped: it keeps its momentum, coasts under the
normal car physics and is drawn in the world until rolling resistance, drag
or an obstacle stops it. The player can walk back and re-enter it if it is
still within reach. Below the bailout speed the same key stops the car and
puts the player down beside it as before.

## The fall model

- Gravity is 9.81 m/s².
- A belly-to-earth body reaches about 55 m/s at sea level. Air density falls
  exponentially with height, so a fall from airliner altitude reaches a
  higher terminal speed (capped at 100 m/s here). Deceleration is capped at
  8 g so a supersonic exit is survivable in game terms.
- Tracking adds a small horizontal acceleration in the facing direction.
- Wind is the planet-frame wind at the body, so freefall and canopy flight
  drift downwind. The HUD shows the wind at the ground below.
- The freefall lasts until the player opens the parachute: there is no
  automatic deployment.
- The canopy inflates over 2.8 s. Its target descent is 5.2 m/s and its
  forward speed through the air is 9.5 m/s; the target is followed at a rate
  that produces a realistic opening shock instead of an instantaneous 50 g
  stop. The HUD shows the opening percentage and the descent rate.
- A flare reduces the descent to about 3.4 m/s for a moment; holding speed up
  increases the glide to about 11.5 m/s and the descent a little.

A canopy landing is safe to about 6.5 m/s of vertical speed. A touchdown on
water is handled separately (below), and a freefall into the ground is not
survivable.

## Landing and water

On land the body keeps its horizontal speed, slows under the normal on-foot
ground friction, takes any hard-landing damage, and is in `Mode::Walk`
immediately. Buildings and tree trunks push the capsule out on the way down;
a descent onto a roof slides down its side rather than entering it.

There is no swimming yet. A water landing is reported by the character
controller; the game washes the body ashore at the nearest dry land within
72 m. In open water, with no land in reach, the player is rescued to the
nearest airfield in the airport database (up to 2,000 km away), so a sea
bailout always has an outcome. The splash plays its own sound, and a very
fast water impact still hurts.

## Unmanned aircraft

The aircraft is marked unmanned on bailout. It gets idle power, a gentle bank
away from the player for the first 50 s, and is then flown by the normal
`Plane::step` with neutral controls. If it crashes while the player is out,
the usual crash scene, fire, debris, splash and audio run, but the player is
not moved into the crash camera and is not marked crashed. The destroyed
aircraft stops being drawn or stepped; the wreck remains.

Saving is paused during the fall or a car bailout: autosave, the menu's save
and the window's close all keep the previous resume snapshot rather than
writing a body in mid-air, in open water or mid-roll. The resume is written
again as soon as the player is back on foot.

## Sources and assumptions

The figures are rounded, commonly published sport-parachuting values, not
measurements of a particular canopy:

- USPA, *Skydiver's Information Manual* (SIM), sections on freefall, canopy
  control and landing priorities: <https://uspa.org/SIM>
- FAA Advisory Circular 105-2E, *Sport Parachuting*:
  <https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_105-2E.pdf>

Assumed for the game: 55 m/s belly-to-earth terminal at sea level (about
120 mph), 5.2 m/s canopy descent and 9.5 m/s forward speed, and a 2.8 s
inflation. The opening-shock cap, the 8 g freefall drag cap, the fully manual
deployment, the flare amounts and the "washed ashore" water outcome are game
decisions, not reference figures.

## Limitations

- Ejection seats are not modelled as a separate body, and the seat itself is
  not drawn.
- Canopy malfunctions, reserve parachutes, and swooping/braked flight are not
  modelled.
- Swimming, drowning and rescue are not implemented; the water outcome is
  deliberately "washed ashore".
- Buildings are solid capsules, not walkable roofs.
- The opening is rate-limited for playability; it is not a data-driven
  canopy certification model.

## Verification

`cargo test --bin gtw bailout`, `cargo test --bin gtw character::tests`,
`cargo test --bin gtw avatar::tests` and `cargo test --test gtw_smoke` cover
the freefall terminal speed, high-altitude thin air, the canopy staying packed
until it is pulled, manual opening and steering, wind drift, land and water
touchdown, hard-landing damage, the character's landing/respawn path, the car
bailout roll and stand-up, and scripted headless bailouts for both the
aircraft and a moving car. Pose tests also cover planted walking feet and the
running flight windows, signed backwards/sideways steps, the developing
freefall arch, full-mesh car-roll/recovery contact, and suspension-line
endpoints from early inflation to full canopy. The aircraft scripted run is:

```sh
cargo run --release -- --headless --flat --start plane --aircraft c172 \
  --lat -34.60 --lon -58.45 --alt 180 --wind 0,0 --bailout 1 --no-save
```

The moving-car run is:

```sh
cargo run --release -- --headless --flat --start car --car-bailout 1 \
  --lat -34.60 --lon -58.45 --no-save
```

For a manual check, fly at 500, 3,000 and 10,000 m, press `F7`, pull at
different heights, steer downwind and upwind, and land on a runway, a field,
a city street and water. Watch the empty aircraft in the chase view while
under canopy. In a car, drive to walking pace, then to speed, and press `E`
or `F7` both times: the slow one is a step out, the fast one leaves the car
rolling and the body tumbling.
