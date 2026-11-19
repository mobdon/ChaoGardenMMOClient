# Stage registration

Run stage-registration STAGE.rel OUTPUT.json with the normal --source-root restriction. The report identifies the pinned module, prolog, engine registration target, descriptor name and startup, teardown and entry callbacks. The descriptor offset is relative to section 5 and callback offsets to section 1. The output manifest retains the source hash.

The installed numbered modules share the same ten-instruction registration prolog. tests/stage_registration_ppc.py independently relocates each module and runs its prolog, capturing the descriptor passed to the engine. All 58 numbered modules match, including the shared stage 20 used by mech Tails in Hero action 7 and mech Eggman in Dark. Stage 42 has a verified registration identity even though it has no mapped terrain in the current catalog. Module count is not a count of unique story layouts or playable levels. This command does not execute any of the three registered callbacks.
