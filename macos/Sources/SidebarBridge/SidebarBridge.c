#include "SidebarBridge.h"

// The last-item sentinel is pointer value 2, not a retainable CF object.
// Pass it in C to avoid Swift ARC trying to retain it.
bool RWSInsertSidebarURL(LSSharedFileListRef list, CFURLRef url) {
    CFStringRef path = CFURLCopyFileSystemPath(url, kCFURLPOSIXPathStyle);
    const void *keys[] = { CFSTR("io.github.ssime-git.RWS.mountPath") };
    const void *values[] = { path };
    CFDictionaryRef properties = CFDictionaryCreate(NULL, keys, values, 1,
        &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
    LSSharedFileListItemRef item = LSSharedFileListInsertItemURL(
        list, kLSSharedFileListItemLast, NULL, NULL, url, properties, NULL);
    CFRelease(properties);
    CFRelease(path);
    if (!item) return false;
    CFRelease(item);
    return true;
}
