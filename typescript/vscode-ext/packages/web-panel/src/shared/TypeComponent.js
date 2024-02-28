"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var react_1 = require("react");
var ASTProvider_1 = require("./ASTProvider");
var Link_1 = require("./Link");
var TypeComponent = function (_a) {
    var typeString = _a.typeString;
    var _b = (0, react_1.useContext)(ASTProvider_1.ASTContext).db, classes = _b.classes, enums = _b.enums;
    var elements = [];
    var regex = /(\w+)/g;
    var lastIndex = 0;
    typeString.replace(regex, function (match, className, index) {
        // Add text before the match as plain string
        if (index > lastIndex) {
            elements.push(typeString.substring(lastIndex, index));
        }
        // Check if the class name matches any in the classes array
        var matchedClass = classes.find(function (cls) { return cls.name.value === className; });
        var matchedEnum = enums.find(function (enm) { return enm.name.value === className; });
        if (matchedClass) {
            elements.push((0, Link_1.default)({ item: matchedClass.name }));
        }
        else if (matchedEnum) {
            elements.push((0, Link_1.default)({ item: matchedEnum.name }));
        }
        else {
            elements.push(className);
        }
        lastIndex = index + match.length;
        return match;
    });
    // Add any remaining text
    if (lastIndex < typeString.length) {
        elements.push(typeString.substring(lastIndex));
    }
    return <>{elements}</>;
};
exports.default = TypeComponent;
