# Shared ring and spring tasks in story levels

A story mount uses each verified SET's own object table. A placement reaches the ordinary ring or one of the two springs only when its native task address and original name match. The local index alone has no meaning across different stages. The ring also rejects the alternate terrain-following path. City Escape's board and music adapters remain stage-specific.

In the supported install, 844 ring placements, 42 spring A placements and 321 spring B placements select these partial callbacks. Six other placed City Escape objects retain their earlier partial callbacks, for 1,213 distinct placements total. The other native behaviors, scheduling, physical player movement and stage clear are open. Supplying an observed contact is not a simulation of that contact.

From the checkout, replay Green Forest layer 0, row 1 (spring B) or row 7 (ring). Use a fresh local output path and a local request using the spring or ring replay schema:

    sa2-assets fan-replay GAME/sonic2app.exe .local/spring.json PC_SA2 "Hero Story Sequence" 13 0 1 REQUEST.json

For a magnetic ring an active frame accepts magnet_players per shielded slot with position, collision_center and speed. When the native task calls CalcAdvanceAsPossible, supply the observed output position as advance_result. The trace distinguishes that external result from the selected player, travel distance argument, timer, spin decision and collision registration that Rust computes. A tie between two shielded players returns to ordinary ring behavior. The strict initial reach is under 50 native units; once the timer is positive the task can continue outside that distance. Neither the CLI nor these calls update a server wallet or a save.

The installed Green Forest test exercises both callback types, magnetic movement requests, ordinary pickup and destruction. The optional compare_magnet_c test compiles the complete o_ring_8 function from the pinned SA2B code used by the fixed-ring reference. The 200 scenarios vary eligibility, equal distances, both sides of the range boundary, a nonzero timer and timer wrap. The step argument is compared bit-for-bit on these host math fixtures. The external movement call is observed, not reconstructed.
