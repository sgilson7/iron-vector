# Brief

Stage 1, written by the person. `C1 Frame the problem and set the rules`, `B1 Establish context`.

## The request, word for word

> use all of the skills and all of the sgilson7.github.io tools, concepts, games, etc, and the games rig riot and Armored core 6 as inspirations. I want to make a single player mission based mech game with a fully operational and fledged garage mode to edit your mech. I want the game to use the builder framework found in the builder repo a few directories up; only clone directories if you do not find them on my computer. use the builder framework to get a test 3D game, then build the MVP for a garage mode for a mech, then a single mission where you have to fly across a map with some buildings, destroy a few helicopters, and then battle another super simple armored core type enemy / enemy mech. please don't stop working until you have a working 3D mvp for me to try of controlling a mech in 3d without lag, deployed to my public github pages, when that is ready then you can build the 2nd stage of the mvp with the garage and level. take advantage of all the  claude skills you have, all the builder documentation, repository examples of the builder framework for making lightweight browser games, presentations like useful prompts for generative ai and ai literacy for game ai to help execute this build. begin working. i approve all public github creation and pages deployment ahead of time, and i approve you to make changes according to your recommendations when you do not know the answer. good luck and i am excited to see the 3d mvp

## The tool

**Iron Vector**: a single-player, mission-based 3D mech game that runs in a desktop browser (keyboard and mouse). You build a mech in a garage from parts, then fly it through a mission. The working title is the agent's choice (see Autonomy). No brand name, character, place or term from the inspiring games appears in anything a player can see.

## Done means

Stage A (the 3D MVP), deployed before stage B starts:

- The page loads from `https://sgilson7.github.io/iron-vector/` with no off-origin request.
- A mech stands in a 3D area with buildings. WASD moves it relative to the camera, the mouse turns the camera, Space ascends, Shift quick-boosts, C toggles glide boost, and the left and right mouse buttons fire.
- The mech collides with the ground and the buildings, and EN (energy) limits ascent and boosting.
- On an ordinary laptop the page holds the display's frame rate, and mouse look responds every frame, not every simulation tick.

Stage B (garage and mission):

- A garage where every slot (head, core, arms, legs, booster, generator, right weapon, left weapon, shoulder weapon) can be swapped, with stats recomputed and a live 3D preview.
- One mission: cross a city, destroy the helicopters, then defeat one enemy mech. The mission ends in success or failure, and you can return to the garage.

## Decisions that stay with the person

- The game's name and any text a player reads can be revised by Sam after deploy.
- Whether any audio ships (none does in this build).

## Autonomy

Sam approved in advance (2026-10-08): the plan, the creation of a public GitHub repository, and deploys to GitHub Pages, at both stage gates. Where the brief is silent, the agent decides, records the decision in `SECOND-ORDER.md`, and continues. The agent may not loosen a Builder requirement or a boundary test to make work pass. It must stop and say so in a worklist row instead. It may not add a server, an account, analytics or any off-origin request.

## Allowed core dependencies

`serde`, `serde_json` (reference default). Nothing else.
