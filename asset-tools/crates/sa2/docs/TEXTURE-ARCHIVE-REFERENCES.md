# Texture and archive readers

Algorithm/layout references inspected November 18, 2026:

- X-Hax/SA3D.Archival `src/SA3D.Archival/PRS.cs` (main): LSB control stream, short/long overlapping copies, zero-word terminator.
- X-Hax/sa_tools master `Libraries/ArchiveLib/Puyo.cs` and `PAK.cs`: GVM big-endian table fields with little-endian chunk lengths; PC PAK variable-length entry table at 0x3d. Entry names and long paths are metadata, never trusted output paths.
- X-Hax/sa_tools master `Libraries/TextureLib/GVR/{GvrTexture,GvrFormats,GvrDataCodec}.cs` and `GvrDataCodecs/`: tile order, AR/GB split RGBA, CMPR block order, RGB5A3 and palette flags.
- X-Hax/sa_tools master `Libraries/TextureLib/TexturePalette.cs`: GVPL length is little endian, fields/colors big endian. SA2 palette formats 0/2 use ARGB1555/ARGB4444, while texture format 5 is RGB5A3.
- Microsoft Direct3D DDS programming guide: legacy DDS header and DXT1/3/5 layout.

The Rust implementation is bounded and independently structured. No external code, executable or copyrighted game bytes are embedded. PRS requires an explicit output budget and a terminator. GVM rejects unknown metadata instead of scanning arbitrary payload bytes for apparent headers. PAK's second length is preserved without guessing its meaning. DDS defaults to the first mip. The texture command accepts `--mip LEVEL` for a declared level of a supported 2D chain; DX10 and unsupported masks fail. GVR preserves all source alpha and returns only the largest mip; mip data remains available in the source chunk, with flags recorded by its header. External palette assignment is scene-dependent and must be explicit. Missing palette mappings fail instead of synthesizing original colors.

RGBA GVR rectangles with one dimension below 4 and the other above 4, single-column 4-bit images, and CMPR images with either dimension below 4 are explicitly unsupported. These shapes are absent from the verified local base-image corpus; their padding is not guessed. Encoded spans are validated before allocating the output image. PNG output remains RGBA without premultiplying or stripping transparent pixels.

`tests/local_texture_parity.rs` provides opt-in byte-for-byte RGBA comparisons to ignored ArchiveTool PNGs, three independently reconstructed child-palette PNGs, and five SA3D decompressed race/Karate resources. Native inputs/expected outputs are read only. These tests require `SA2_GAME_PATH` (installation root) and `SA2_LOCAL_ROOT` (ignored local data root); they contain no game content.

Verified November 18, 2026 with the ignored local corpus: **400 RGBA images matched every byte**, including **883,883 nonopaque source pixels**. Every nonindexed texture in the selected archives had a required PNG reference; missing references fail the test. Eighteen external-palette Chao images were deferred in the first pass; the three known child banks were subsequently verified, leaving fifteen palette assignments unknown.

| Archive | Exact RGBA images |
| --- | ---: |
| al_body | 125 plus 3 original GVP child-bank images |
| al_eye | 13 |
| al_icon | 58 |
| al_stg_neut_tex | 12 |
| al_stg_hero_tex | 37 |
| al_stg_dark_tex | 18 |
| al_neut_obj_tex | 7 |
| e0207texture | 86 |
| al_tex_common | 7 |
| al_tex_chao | 10 |
| al_tex_ent_common_en | 5 |
| al_tex_odekake_menu_en | 7 |
| al_tex_cave_menu_en | 3 |
| al_common_tex | 9 |

The ordered source-corpus SHA-256 (archive names plus each source digest, followed by original `al_nc00.GVP` name/digest) is `f0a4f6f35dc4c419c8460dc3c2da1e95223bc3bf79868428104ebc5e52f21d7d`. The ordered pixel-evidence digest (source digest, entry name, RGBA digest for each comparison, followed by GVP-backed child images) is `8830e14fa533c98d651a0af233c07775f76497f4e1878a9ab13bd52541c87651`. These are reproducible aggregate receipts, not hashes of a single file. The test prints every archive's individual source hash.

Five race/Karate PRS decompressions matched existing SA3D bytes. The installed `event/prs/e0360texture.pak` has SHA-256 `e0a18c1f4e55b4fcd8cc8f79401a9fc660a5d9228eaf686ffbfe1b1571bed80a`; all 118 bounded payloads matched independently supplied loose installation files exactly. Their ordered SHA-256 digest-of-digests is `5071869fbe6d540bbaedce4e8369cd17983039220a59cab215b033c07d377708`. All 117 DDS base images match Pillow 13.0.0 decoding the loose files to RGBA. Each verification record concatenates the binary source SHA-256, little-endian 32-bit width and height, and binary RGBA SHA-256. Sorting the records bytewise and hashing their concatenation gives `89c0c5fe7ecf0215d05bf31b74bf910c6d370a91fee399f7411d47974b957a49`. DXT3/5 have synthetic alpha/interpolation checks, not verified native examples in this corpus. No complete gameplay, original shader, dynamic palette selection, UI behavior or GVR mip-chain conversion is claimed. All 856 DDS levels in the 117-file corpus matched Pillow when each level was presented for independent decoding.
