#include "SidebarBridge.h"

// The last-item sentinel is pointer value 2, not a retainable CF object.
// Pass it in C to avoid Swift ARC trying to retain it.
bool RWSInsertSidebarURL(LSSharedFileListRef list, CFURLRef url) {
    LSSharedFileListItemRef item = LSSharedFileListInsertItemURL(
        list, kLSSharedFileListItemLast, NULL, NULL, url, NULL, NULL);
    if (!item) return false;
    CFRelease(item);
    return true;
}
