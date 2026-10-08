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

## Milestone 2 request, word for word (2026-10-08)

> ok, now I want the garage mode built out, where you can swap pieces with unlocked ones and see then change in real time. then when you press the mission select button, a little animation rendered in engine of the camera sweeping to the first person view from the cockpit, and the inside of the cock pit lights up with a 3d mission selection window, that has missions laid out in a hasse diagram like slushline and vagrancy, with scenes describing each mission you will go to; this time, each mission is a differnet planet in space like star fox 64, and each planet is a fixed map. on each map, there are 4-8 different missions like super mario 64; so on planet one, it could be a normal industrial planet like corneria, where mission 1 is destroy helicopters, mission 2 is beat your friend in a friendly mech race, mission 3 is defeating the invading army (i.e the army attacks buildings, if too many are destroyed you lose) and mission 4 is defeating an enemy mech thats about as dangerous as you are; for control of the enemy mechs, use the sophistaced behavior tree set up from vagrancy for ai control of the enmy mechs, and build a new set of behaviors and keys to execute them that can be arragned into behavior trees. then planet 2 unlocks when you've beaten the first 2 missions, then planet 2 has 4 missions, planet 3 unlocks after 5 total completed missions, then planet 4 opens up after 8 total completed missions, etc. each mission you complete on a planet should give you a piece of gear for your mech, and each mission should have a + completion reward that is revealed after the first time you complete the mission, that gets you another piece of gear. for mission 1, imagine the plus rank is achieved by destroying both helicopters with under 10 bullets spent, so like constraints on solving the originally designed mission. go ahead and create 5 planets of increasing difficulty, each planet should be very distinct from the last, with planets 3, 4, 5 having a unique gameplay gimmick, like planet 3 being a giant space tower built from the old remains of a planet, so everything goes in one direction along the tube and the gravity is like a halo ring, so the cetrifugal force throws you on the outside of the ring. please execute this, keep in mind keeping each planet visually distinct and easily seeable on the 3d star map mission map. dont stop till its deployed live for my review

Autonomy for milestone 2: the request says to execute and deploy for review, so the plan is approved in advance and a Pages deploy is authorised once `./scripts/check.sh` passes.
