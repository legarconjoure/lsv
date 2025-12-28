# Image Preview Support

## Summary

Added native image preview support using the Kitty/iTerm2/Sixel protocols via `ratatui-image`.

## Changes

### Dependencies
- Updated `ratatui` from `0.29` to `0.30`
- Added `ratatui-image` `10.0` for terminal image protocol support
- Added `image` `0.25` for image decoding

### Code Changes

**`src/app/state.rs`**
- Added `PreviewContent` enum to differentiate text vs image previews
- Extended `PreviewState` with `content: Option<PreviewContent>`
- Added `image_state: Option<Box<dyn Any>>` to `App` for protocol state storage

**`src/ui/preview.rs`**
- Modified `draw_preview_panel` to handle image vs text preview branching
- Added `is_image_file()` helper detecting image extensions
- Updated `run_previewer()` to return `(Option<Vec<String>>, Option<PreviewContent>)`
- Images detected by extension bypass Lua previewer

**`src/ui/image_preview.rs`** (new)
- Implements `draw_image_preview()` using `ratatui_image::StatefulImage`
- Uses halfblocks protocol (most compatible, works in any terminal)
- Caches protocol state in `app.image_state` to avoid re-decoding
- Graceful error handling with styled error messages

**`src/app/preview_ctrl.rs`**
- Clear `image_state` when preview is invalidated

**`examples/config/init.lua`**
- Removed `viu` ASCII art fallback for images
- Now returns `nil` for image extensions, letting native preview handle them

## Supported Image Formats
jpg, jpeg, png, gif, bmp, webp, tiff, tif, ico

## Terminal Compatibility
- **Halfblocks mode** (current): Works in any terminal with Unicode support
- **Future**: Can detect and use Kitty/iTerm2/Sixel protocols for better quality

## Testing
- All existing tests pass
- Release build successful
- Ready for manual testing with image files
