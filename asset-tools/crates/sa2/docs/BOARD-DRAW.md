# City Escape board display callbacks

The Rust planner mirrors the control flow of ObjectBoardDisp and ObjectBoardDispShad in the local SA2B reference src/stg13_cityescape/o_ce_board.c (SHA-256 d06037c2cb3fcdc637fcf85951a339b0698d0ce98dbece0d4f8908a280b26f54). It returns two independent operation lists. Their order relative to each other belongs to the external scheduler. Character IDs come from include/samt/sonic/player.h.

Supply the board task position, XYZ angles in native angle units, mode, cached action/frame, TWO_PLAYER flag, and the board slot's current player observation. A missing player task yields two empty lists. The selected texture, display object and unused third object are source symbols; the shadow always uses lbl_13_data_1063DC.

Supply observations of the first word of required entries of lbl_801E5D28._4 as (index, pointer) pairs. Zero means an observed null entry; an absent or duplicate required row is an error. No host pointer is dereferenced. Only attached mode reads the current action + 24; only the blend branch reads cached action + 24. For a signed action outside the represented 300 entries, the planner reports a bounds error instead of extrapolating native memory.

DrawRigid is fn_8011DF44; DrawMotion is fn_8011E214; DrawBlend is fn_8011E19C; DrawControlObject is fn_8011E244. RotateObjectRoot uses the native selected object's own angles with njRotateEx order 0. InstallMatrixCaptureCallback registers o_ce_board_4 via fn_8011C3A0; only an actual invocation would capture via njGetMatrix. CaptureMatrix records the separate explicit capture, and SetCapturedMatrix reads lbl_13_bss_0 even if the primary draw was skipped. The planner does not synthesize an identity matrix. The control pass uses mask 0x2400 and lbl_13_data_106534.

ShadowBegin represents fn_13_1863C(work, scale, zero, &position, 0, 0); ShadowEnd is fn_13_18978(work); ShadowFinish is fn_13_18AE8(kind, &position, value, work). The work pointer belongs to the original board task. Native model data, motion rows, interpolation, callback invocation, matrix contents, shadow helper implementation and rendered pixels are not provided by these decisions.
