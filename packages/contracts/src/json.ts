const INTEGER_PATTERN = /^-?(?:0|[1-9]\d*)$/u;

export class JsonCodecError extends SyntaxError {}

/** Parses JSON without first narrowing integer tokens through IEEE-754 numbers. */
export function parseJsonWithBigInts(source: string): unknown {
  return new Parser(source).parse();
}

/** Serializes bigint values as JSON number tokens, never quoted strings. */
export function stringifyJsonWithBigInts(value: unknown): string {
  if (value === null) return "null";
  switch (typeof value) {
    case "bigint":
      return value.toString();
    case "boolean":
    case "number":
    case "string": {
      return JSON.stringify(value);
    }
    case "object":
      if (Array.isArray(value)) {
        return `[${value.map((entry) => stringifyJsonWithBigInts(entry)).join(",")}]`;
      }
      return `{${Object.entries(value)
        .filter(([, entry]) => entry !== undefined)
        .map(([key, entry]) => `${JSON.stringify(key)}:${stringifyJsonWithBigInts(entry)}`)
        .join(",")}}`;
    default:
      throw new JsonCodecError(`Unsupported JSON value: ${typeof value}`);
  }
}

class Parser {
  readonly #source: string;
  #index = 0;

  constructor(source: string) {
    this.#source = source;
  }

  parse(): unknown {
    const value = this.#parseValue();
    this.#skipWhitespace();
    if (this.#index !== this.#source.length) this.#fail("Unexpected trailing input");
    return value;
  }

  #parseValue(): unknown {
    this.#skipWhitespace();
    const token = this.#source[this.#index];
    if (token === '"') return this.#parseString();
    if (token === "{") return this.#parseObject();
    if (token === "[") return this.#parseArray();
    if (token === "t") return this.#parseLiteral("true", true);
    if (token === "f") return this.#parseLiteral("false", false);
    if (token === "n") return this.#parseLiteral("null", null);
    if (token === "-" || (token !== undefined && token >= "0" && token <= "9")) {
      return this.#parseNumber();
    }
    return this.#fail("Expected a JSON value");
  }

  #parseObject(): Record<string, unknown> {
    this.#index++;
    const result: Record<string, unknown> = {};
    this.#skipWhitespace();
    if (this.#consume("}")) return result;
    for (;;) {
      this.#skipWhitespace();
      if (this.#source[this.#index] !== '"') this.#fail("Expected an object key");
      const key = this.#parseString();
      this.#skipWhitespace();
      if (!this.#consume(":")) this.#fail("Expected ':' after object key");
      Object.defineProperty(result, key, {
        configurable: true,
        enumerable: true,
        value: this.#parseValue(),
        writable: true,
      });
      this.#skipWhitespace();
      if (this.#consume("}")) return result;
      if (!this.#consume(",")) this.#fail("Expected ',' or '}'");
    }
  }

  #parseArray(): unknown[] {
    this.#index++;
    const result: unknown[] = [];
    this.#skipWhitespace();
    if (this.#consume("]")) return result;
    for (;;) {
      result.push(this.#parseValue());
      this.#skipWhitespace();
      if (this.#consume("]")) return result;
      if (!this.#consume(",")) this.#fail("Expected ',' or ']'");
    }
  }

  #parseString(): string {
    const start = this.#index++;
    while (this.#index < this.#source.length) {
      const character = this.#source[this.#index++];
      if (character === '"') {
        try {
          return JSON.parse(this.#source.slice(start, this.#index)) as string;
        } catch {
          return this.#fail("Invalid JSON string");
        }
      }
      if (character === "\\") this.#index++;
      else if (character !== undefined && character < " ") this.#fail("Control character in string");
    }
    return this.#fail("Unterminated string");
  }

  #parseNumber(): number | bigint {
    const remaining = this.#source.slice(this.#index);
    const match = /^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/u.exec(remaining);
    if (match === null) return this.#fail("Invalid JSON number");
    const token = match[0];
    this.#index += token.length;
    if (INTEGER_PATTERN.test(token)) return BigInt(token);
    const value = Number(token);
    if (!Number.isFinite(value)) return this.#fail("JSON number is outside the finite range");
    return value;
  }

  #parseLiteral<T>(token: string, value: T): T {
    if (!this.#source.startsWith(token, this.#index)) this.#fail(`Expected '${token}'`);
    this.#index += token.length;
    return value;
  }

  #skipWhitespace(): void {
    for (;;) {
      const character = this.#source[this.#index];
      if (character !== " " && character !== "\t" && character !== "\n" && character !== "\r") return;
      this.#index++;
    }
  }

  #consume(token: string): boolean {
    if (this.#source[this.#index] !== token) return false;
    this.#index++;
    return true;
  }

  #fail(message: string): never {
    throw new JsonCodecError(`${message} at offset ${String(this.#index)}`);
  }
}
