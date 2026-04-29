import { Separator, type Theme } from '@inquirer/core';
import type { PartialDeep } from '@inquirer/type';
type RawlistTheme = {
    style: {
        description: (text: string) => string;
    };
};
type Choice<Value> = {
    value: Value;
    name?: string;
    short?: string;
    key?: string;
    description?: string;
};
declare const _default: <Value>(config: {
    message: string;
    choices: readonly (Separator | Value | Choice<Value>)[];
    loop?: boolean | undefined;
    theme?: PartialDeep<Theme<RawlistTheme>> | undefined;
    default?: NoInfer<Value> | undefined;
}, context?: import("@inquirer/type").Context) => Promise<Value>;
export default _default;
export { Separator } from '@inquirer/core';
