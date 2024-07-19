"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.BamlStream = void 0;
class BamlStream {
    ffiStream;
    partialCoerce;
    finalCoerce;
    ctxManager;
    task = null;
    eventQueue = [];
    constructor(ffiStream, partialCoerce, finalCoerce, ctxManager) {
        this.ffiStream = ffiStream;
        this.partialCoerce = partialCoerce;
        this.finalCoerce = finalCoerce;
        this.ctxManager = ctxManager;
    }
    async driveToCompletion() {
        try {
            this.ffiStream.onEvent((err, data) => {
                if (err) {
                    return;
                }
                else {
                    this.eventQueue.push(data);
                }
            });
            const retval = await this.ffiStream.done(this.ctxManager);
            return retval;
        }
        finally {
            this.eventQueue.push(null);
        }
    }
    driveToCompletionInBg() {
        if (this.task === null) {
            this.task = this.driveToCompletion();
        }
        return this.task;
    }
    async *[Symbol.asyncIterator]() {
        this.driveToCompletionInBg();
        while (true) {
            const event = this.eventQueue.shift();
            if (event === undefined) {
                await new Promise((resolve) => setTimeout(resolve, 100));
                continue;
            }
            if (event === null) {
                break;
            }
            if (event.isOk()) {
                yield this.partialCoerce(event.parsed());
            }
        }
    }
    async getFinalResponse() {
        const final = await this.driveToCompletionInBg();
        return this.finalCoerce(final.parsed());
    }
}
exports.BamlStream = BamlStream;
