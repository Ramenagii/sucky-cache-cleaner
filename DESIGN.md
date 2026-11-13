# Sucky Desktop Design System — Clucky Edition

Clucky's warm, chunky, game-y confidence, applied to Sucky's app family. Same fish, new jacket: black chunky type on warm paper, flame-orange accents, zero enterprise.

## Vibe
Cozy farm-game UI, not a productivity tool. Thick rounded type, fully-rounded pill buttons, one confident orange. Every surface looks handheld and touchable. Calm, generous, a little goofy — never sterile.

## Color
| Token | Hex | Use |
|---|---|---|
| `--paper` | `#EDEDE7` | App background (warm light gray, Clucky canvas) |
| `--panel` | `#FFFFFF` | Cards, surfaces |
| `--ink` | `#141414` | Headlines/wordmark — Clucky near-black |
| `--text` | `#3A3A35` | Body text |
| `--muted` | `#7F7E77` | Captions, secondary |
| `--flame` | `#FF9E00` | Primary accent highlights, mascot accents |
| `--flame-yellow` | `#FFC233` | Flame fill, celebratory moments |
| `--ember` | `#C2410C` | Primary buttons (Clucky burnt orange), key actions |
| `--ember-dark` | `#9A3412` | Hover/pressed |
| `--leaf` | `#2FA84F` | Success, safe, confirmed (streak green) |
| `--red` | `#D64533` | Destructive only |
| `--hairline` | `#E0E0DA` | Borders, dividers |

Rules: one orange per screen. Yellow = celebration, never structure. Red = irreversible only. Warm gray canvas, white cards, black type. No blues, no purples, no cold grays.

## Typography
- Display/wordmark/headlines: **Baloo 2 ExtraBold (800)** — the Clucky look. Fallbacks: Anybody 800, Quicksand 700.
- Body: **Nunito Sans 400/700**. Fallbacks: Quicksand.
- Numbers (stats, streaks, sizes): Baloo 2 700.
- Scale: 34 display / 24 title / 17 button / 15 body / 12 caption / 11 micro-caps muted (letterspaced).

## Shape & elevation
- Fully rounded pills everywhere (`round_full`): buttons are stadium pills, not rectangles with radius.
- Cards: 28px radius, pure white `#FFFFFF`, one soft shadow `0 6px 24px rgba(20,20,20,0.10)`.
- Inputs: 16px radius. Progress tracks: 6px, round caps.
- Chips/streak dots: circles. Selected-day dot = leaf green.

## Components
- **Primary button**: ember `#C2410C` fill, white bold Baloo 2 800, stadium pill, big padding. Hover → ember-dark.
- **Secondary button**: white fill, ink 800 text, hairline border.
- **Tertiary/ghost**: text-only, ink 700.
- **Destructive**: red pill. Only irreversible actions.
- **Card**: white, 28px radius, soft shadow, micro-caps muted section label.
- **TANK STATUS / STREAK card**: signature floating card — big Baloo 2 number, label, thin progress bar, 2-3 rows. (Clucky's "Your little farm" and "TANK STATUS" cards.)
- **Suggestion rows**: flame or leaf dot + one line of evidence + reason. One-click "Mark all suggested" ember pill.
- **Toast**: near-black pill, white Baloo 2 700 text, bottom-center, fades.
- **Nav bar**: top bar with mascot + wordmark left; right = one ember pill action.

## Mascot
Spotted pleco in an **ember-orange apron** (fish identity kept, palette swapped from teal to flame). Flat storybook style: bronze body, dark spots, white eye with black pupil, simple smile. Used in nav bar and hero. Clucky-family warmth.

## Layout
- Two-zone hero: left = big Baloo headline + subline + pill actions; right = mascot + floating stat card.
- One primary card per band below the hero. Max width ~880px. Content sits on white cards on paper canvas.
- Desktop first; same pills/cards reflow down.

## Voice
First person, brief, warm, concrete. "Sucky spotted 2.3 GB — safe to scrub." Never "Items selected for deletion: 3." Errors name cause + next click, never a stack trace. Playful, but the verb (delete) is always honest.
