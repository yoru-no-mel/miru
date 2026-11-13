#import <AppKit/AppKit.h>
#import <Carbon/Carbon.h>

extern void miru_open_document(const char *path);

@interface MiruOpenDocument : NSDocument
@end

@implementation MiruOpenDocument
- (BOOL)readFromURL:(NSURL *)url
             ofType:(NSString *)typeName
              error:(NSError **)outError {
    (void)typeName;
    (void)outError;

    if (!url.isFileURL || !url.fileSystemRepresentation) {
        return NO;
    }
    miru_open_document(url.fileSystemRepresentation);
    return YES;
}
@end

@interface MiruOpenDocumentHandler : NSObject
- (void)openDocuments:(NSAppleEventDescriptor *)event
        withReplyEvent:(NSAppleEventDescriptor *)reply;
@end

@implementation MiruOpenDocumentHandler
- (void)openDocuments:(NSAppleEventDescriptor *)event
        withReplyEvent:(NSAppleEventDescriptor *)reply {
    (void)reply;

    NSAppleEventDescriptor *documents =
        [event paramDescriptorForKeyword:keyDirectObject];
    if (!documents) {
        return;
    }

    BOOL isList = documents.descriptorType == typeAEList;
    NSInteger count = isList ? documents.numberOfItems : 1;
    for (NSInteger index = 1; index <= count; ++index) {
        NSAppleEventDescriptor *document =
            isList ? [documents descriptorAtIndex:index] : documents;
        NSURL *url = document.fileURLValue;
        if (!url && document.stringValue.length > 0) {
            url = [NSURL fileURLWithPath:document.stringValue];
        }
        if (url.isFileURL && url.fileSystemRepresentation) {
            miru_open_document(url.fileSystemRepresentation);
        }
    }
}
@end

void miru_register_open_handler(void) {
    static MiruOpenDocumentHandler *handler;
    // Keep the document subclass in the linked executable. It is looked up by
    // name from Info.plist, so the linker otherwise sees no direct reference
    // and may strip it from the static Objective-C archive.
    (void)[MiruOpenDocument class];
    if (!handler) {
        handler = [MiruOpenDocumentHandler new];
    }
    [[NSAppleEventManager sharedAppleEventManager]
        setEventHandler:handler
           andSelector:@selector(openDocuments:withReplyEvent:)
         forEventClass:kCoreEventClass
            andEventID:kAEOpenDocuments];
}
