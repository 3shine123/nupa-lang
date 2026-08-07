#include <Cocoa/Cocoa.h>

/* Activate the macOS app so the window gets keyboard focus
   when launched from a terminal. */
void nk_app_activate(void) {
    [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
    [NSApp activateIgnoringOtherApps:YES];
}