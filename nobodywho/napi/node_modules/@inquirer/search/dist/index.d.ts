import { Separator, type Theme } from '@inquirer/core';
import type { PartialDeep } from '@inquirer/type';
type SearchTheme = {
    icon: {
        cursor: string;
    };
    style: {
        disabled: (text: string) => string;
        searchTerm: (text: string) => string;
        description: (text: string) => string;
        keysHelpTip: (keys: [key: string, action: string][]) => string | undefined;
    };
};
type Choice<Value> = {
    value: Value;
    name?: string;
    description?: string;
    short?: string;
    disabled?: boolean | string;
    type?: never;
};
declare const _default: <Value>(config: {
    message: string;
    source: (term: string | undefined, opt: {
        signal: AbortSignal;
    }) => readonly (string | Separator)[] | readonly (Separator | Choice<Value>)[] | Promise<readonly (string | Separator)[]> | Promise<readonly (Separator | Choice<Value>)[]>;
    validate?: ((value: Value) => boolean | string | Promise<string | boolean>) | undefined;
    pageSize?: number | undefined;
    default?: NoInfer<Value> | undefined;
    theme?: PartialDeep<Theme<SearchTheme>> | undefined;
}, context?: import("@inquirer/type").Context) => Promise<Value>;
export default _default;
export { Separator } from '@inquirer/core';
