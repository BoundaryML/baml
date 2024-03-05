type Scope = string[];

interface DeserializerErrorInterface {
    scope: Scope;
    message: string;
    toWarning(): DeserializerWarning;
}

class DeserializerError implements DeserializerErrorInterface {
    
    constructor(private _scope: Scope, private _message: string) {
    }

    get message(): string {
        return this._message;
    }

    get scope(): Scope {
        return this._scope;
    }

    toString(): string {
        return this.scope.length === 0 ? `Error: ${this._message}` : `Error in ${this.scope.join('.')}: ${this._message}`;
    }

    toWarning(): DeserializerWarning {
        return new DeserializerWarning(this.scope, this._message);
    }
}

class DeserializerWarning {
    constructor(private _scope: Scope, private _message: string) {
    }


    get scope(): Scope {
        return this._scope;
    }

    toString(): string {
        return this.scope.length === 0 ? `Warning: ${this._message}` : `Warning in ${this.scope.join('.')}: ${this._message}`;
    }
}

class DeserializerException extends Error {
    private errors: DeserializerError[];
    private warnings: DeserializerWarning[];
    private rawString: string;

    constructor(errors: DeserializerError[], warnings: DeserializerWarning[], rawString: string) {
        super(`Failed to Deserialize: (${errors.length} errors) (${warnings.length} warnings)`);
        this.errors = errors;
        this.warnings = warnings;
        this.rawString = rawString;
    }

    toString(): string {
        let output: string[] = [this.message];
        let items = [...this.errors, ...this.warnings];
        items.sort((a, b) => b.scope.length - a.scope.length);
        items.forEach(i => {
            output.push('------');
            output.push(i.toString());
        });
        output.push('------');
        output.push('Raw:');
        output.push(this.rawString);
        return output.join('\n');
    }
}

class Diagnostics {
    private errors: DeserializerError[] = [];
    private warnings: DeserializerWarning[] = [];
    private rawString: string;
    private scope: Scope = [];
    private scopeErrors: { [key: string]: DeserializerError[] } = {};

    constructor(rawString: string) {
        this.rawString = rawString;
    }

    pushScope(scope: string): void {
        this.scope.push(scope);
        this.scopeErrors[this.scope.join('.')] = [];
    }

    popScope(errorsAsWarnings: boolean): void {
        let prevScopeKey = this.scope.join('.');
        this.scope.pop();
        // If there are any errors, convert them to warnings.
        this.scopeErrors[prevScopeKey]?.forEach(error => {
            if (errorsAsWarnings) {
                this.pushWarning(error.toWarning());
            } else {
                this.pushError(error);
            }
        });
    }

    private pushError(error: DeserializerError): void {
        let key = this.scope.join('.');
        if (key) {
            this.scopeErrors[key]?.push(error);
        } else {
            this.errors.push(error);
        }
    }

    private pushWarning(warning: DeserializerWarning): void {
        this.warnings.push(warning);
    }

    toException(): void {
        if (this.errors.length > 0) {
            throw new DeserializerException(this.errors, this.warnings, this.rawString);
        }
    }

    pushUnknownWarning(message: string): void {
        this.pushWarning(new DeserializerWarning(this.scope, message));
    }

    pushEnumError(enumName: string, value: any, expected: string[]): void {
        this.pushError(new DeserializerError(this.scope, `Failed to parse \`${value}\` as \`${enumName}\`. Expected one of: ${expected.join(', ')}`));
    }

    pushUnknownError(message: string): void {
        this.pushError(new DeserializerError(this.scope, message));
    }
}

export { DeserializerError, DeserializerWarning, DeserializerException, Diagnostics };
