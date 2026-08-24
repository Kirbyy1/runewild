# Simplified Texture System - Visual Test Checklist

**Build Status:** ✅ SUCCESSFUL  
**Code Quality:** ✅ Formatted (cargo fmt)  
**Lint Status:** ✅ Passed (cargo clippy)

## System Configuration

- Texture Atlas: **64×32 pixels** (2 tiles × 32×32)
- Tile 0: **GRASS_TOP** (forest green #228B22)
- Tile 1: **DIRT** (brown #8B4513)
- Pattern: **8×8 checkerboard** in each tile
- Filtering: Default Bevy (to be tuned)

## Pre-Game Checks

```bash
# Run the game with full output
cargo run --release

# Watch for this line in console:
# ✓ SIMPLIFIED TEXTURE ATLAS INITIALIZED
```

## IN-GAME TEST SEQUENCE

### Test 1: Grass Top (MOST IMPORTANT)
**Action:** Stand on grass block, look straight down  
**Expected:** Grass top face shows FOREST GREEN with 8×8 checkerboard  
**Result:** ✅ PASS / ❌ FAIL

**If FAIL:** Texture system is not sampling correctly
- Check console for "SIMPLIFIED TEXTURE ATLAS INITIALIZED"
- Check if blocks are still solid flat colors
- Add logging to texture_loader.rs and greedy.rs

### Test 2: Grass Side (IMPORTANT)
**Action:** Find a cliff with grass above dirt, look at the transition  
**Expected:**
- Top face: FOREST GREEN checkerboard
- Side faces: BROWN checkerboard  
**Result:** ✅ PASS / ❌ FAIL

**If FAIL:** Face mapping is incorrect
- Verify get_texture_coords() in texture_atlas.rs
- Check that grass sides return (1,0) not (0,0)

### Test 3: Dirt Block (IMPORTANT)
**Action:** Break a block and expose dirt, look at all six faces  
**Expected:** All faces are BROWN with 8×8 checkerboard  
**Result:** ✅ PASS / ❌ FAIL

**If FAIL:** Dirt mapping may be wrong
- Verify all BlockType::Dirt faces return (1,0)

### Test 4: Texture Stability (FLICKERING CHECK)
**Action:** Slowly walk around terrain and watch for shimmer  
**Expected:** No flickering, colors stable  
**Duration:** 30 seconds of observation  
**Result:** ✅ NO FLICKER / ⚠️ MINOR SHIMMER / ❌ SEVERE FLICKERING

**If SEVERE FLICKERING:**
- Look for overlapping faces at chunk boundaries
- Check for z-fighting (faces too close)
- Investigate should_emit_face() logic

### Test 5: Distance Rendering (FILTERING CHECK)
**Action:** Move ~20 blocks away from terrain, observe appearance  
**Expected:**
- Checkerboard still visible at distance
- Pattern remains stable when camera moves
- No rainbow shimmer or aliasing  
**Result:** ✅ STABLE / ⚠️ SOME SHIMMER / ❌ ALIASING ARTIFACTS

**If ALIASING:**
- Mipmap filtering may need adjustment
- Consider different sampler settings

### Test 6: UV Tiling (GREEDY MESHING CHECK)
**Action:** Find a large flat surface (big grass field or cliff face)  
**Expected:**
- Checkerboard repeats smoothly across merged blocks
- No stretching of pattern
- Pattern is NOT rotated or mirrored  
**Result:** ✅ CORRECT TILING / ⚠️ SLIGHT DISTORTION / ❌ OBVIOUS STRETCHING

**If STRETCHING:**
- UV calculation in greedy.rs may be wrong
- Verify tile_size = 1.0 / tiles_per_row = 0.5
- Check four UV corners calculation

### Test 7: Chunk Boundaries (BOUNDARY CHECK)
**Action:** Walk across chunk boundaries (every 16 blocks by default)  
**Expected:**
- No visible seams
- No duplicate faces
- Textures align perfectly  
**Result:** ✅ SEAMLESS / ⚠️ MINOR SEAMS / ❌ OBVIOUS GAPS

**If GAPS:**
- Chunk boundary generation may have issues
- Check should_emit_face() at boundaries

### Test 8: Close-Up Inspection (MAGNIFICATION CHECK)
**Action:** Stand touching a block face, zoom in very close  
**Expected:**
- Checkerboard pattern is crisp and clear
- Individual 8×8 squares are visible
- No blurriness  
**Result:** ✅ SHARP / ⚠️ SLIGHTLY SOFT / ❌ VERY BLURRY

**If BLURRY:**
- Magnification filtering may need to be nearest-neighbor
- May need shader adjustment

## PASS/FAIL CRITERIA

**System is WORKING if:**
- ✅ Tests 1-3 all PASS (texture appears with correct color and pattern)
- ✅ Test 4 shows NO FLICKER or only minor stable shimmer
- ✅ Test 5 shows stable distance rendering
- ✅ Test 6 shows correct tiling

**System needs DEBUGGING if:**
- ❌ Tests 1-3 show solid flat colors (not checkerboard)
- ❌ Tests 1-3 show wrong colors or no texture
- ❌ Test 4 shows severe flickering
- ❌ Test 6 shows stretching

## NEXT ACTIONS

### If All Tests PASS ✅
1. Document results
2. Create real texture PNG files:
   - grass_top.png (32×32 with actual artistic detail)
   - dirt.png (32×32 with soil/rock detail)
3. Update texture_loader.rs to load from files instead of procedural
4. Test with real textures
5. Add texture variants (grass_01, grass_02, grass_03, etc.)
6. Implement deterministic variant selection

### If Tests FAIL ❌
1. Identify which tests fail
2. Add logging to diagnose:
   - Add println! in get_texture_coords()
   - Add println! in push_face() to log UV values
   - Add println! in should_emit_face() to check face generation
3. Verify texture_atlas.rs mapping is correct
4. Verify greedy.rs UV calculation
5. Check materials.rs texture handle assignment
6. Re-run tests after fixes

## Debug Information Locations

**Texture Loading:** src/rendering/texture_loader.rs line 73-131 (console output)
**UV Mapping:** src/world/meshing/greedy.rs line 91-161  
**Face Selection:** src/world/meshing/greedy.rs line 81-89  
**Material Setup:** src/world/chunk_manager.rs line 203-232  
**Block Texture Map:** src/rendering/texture_atlas.rs line 7-27

## Performance Notes

Current implementation should have **zero performance impact** compared to vertex-color-only system:
- Same mesh generation
- Same number of triangles
- Just using texture sampling instead of vertex colors
- No extra draw calls (single atlas for all blocks)

## Time Estimate

- Visual tests: ~5-10 minutes
- Diagnosis (if needed): ~10-20 minutes
- Fixes (if needed): ~15-30 minutes

---

**Ready to test! Run `cargo run --release` and report results.**
