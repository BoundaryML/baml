"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Extension = void 0;
var vscode_1 = require("vscode");
var Extension = /** @class */ (function () {
    function Extension(ctx) {
        this.ctx = ctx;
    }
    Extension.getInstance = function (ctx) {
        if (!Extension.instance && ctx) {
            Extension.instance = new Extension(ctx);
        }
        return Extension.instance;
    };
    Object.defineProperty(Extension.prototype, "isProductionMode", {
        /**
         * Check if the extension is in production/development mode
         */
        get: function () {
            return this.ctx.extensionMode === vscode_1.ExtensionMode.Production;
        },
        enumerable: false,
        configurable: true
    });
    return Extension;
}());
exports.Extension = Extension;
