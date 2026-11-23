"""Export Sonic's Light Shoes as one rigid shoe per foot, the way the hub made
Shadow's Air Shoes: his own heel/toe mesh from his body model (SONICMDL 0,
right heel node 13 / left 23) with the Light Shoes item (SONICMDL 13 right,
14 left) on it, through scripts/convert-upgrade-assets.py's own convert().

Run after scripts/extract-upgrade-assets.js has split SONICMDL and SONICTEX
(.local/extracted/upgrades/sonicmdl, .local/extracted/textures/sonictex):

  blender --background --factory-startup --python scripts/convert-light-shoes.py -- <checkout with .local>

Output: <checkout>/.local/imports/sonic-light-shoes-{right,left}.glb. Then
scripts/convert-hub-shoe-to-ninja.py makes the .njsh the game reads from
/mmo-assets/. All of it is game-derived: local only, never commit it.
unverified: that the item is modelled in the heel node's frame (assumed, as
Shadow's Air Shoes overlay is); check the result on the foot.
"""
import hashlib
import importlib.util
import sys
from pathlib import Path

root = Path(sys.argv[sys.argv.index('--') + 1]).resolve()
spec = importlib.util.spec_from_file_location('upgrade_assets', root / 'scripts/convert-upgrade-assets.py')
upgrade_assets = importlib.util.module_from_spec(spec)
spec.loader.exec_module(upgrade_assets)


def sha(path):
    return hashlib.sha256((root / path).read_bytes()).hexdigest()


BODY = '.local/extracted/upgrades/sonicmdl/0.sa2mdl'
for side, model, foot in (('right', 13, 13), ('left', 14, 23)):
    native = f'.local/extracted/upgrades/sonicmdl/{model}.sa2mdl'
    record = upgrade_assets.convert({
        'name': f'sonic-light-shoes-{side}', 'itemId': 'sonic-light-shoes', 'side': side, 'slot': 'footwear',
        'archive': 'sonicmdl', 'modelId': model, 'textures': 'sonictex',
        'nativeModel': native, 'nativeSha256': sha(native),
        'baseBoot': {'nativeModel': BODY, 'modelId': 0, 'footNode': foot, 'nativeSha256': sha(BODY),
                     'evidence': 'Sonic and Shadow share the 62-node skeleton: right heel 13 / toe 14, left heel 23 / toe 24.'},
    })
    print('LIGHT_SHOES', side, record['meshCount'], 'meshes', record['vertices'], 'vertices', record['bytes'], 'bytes')
