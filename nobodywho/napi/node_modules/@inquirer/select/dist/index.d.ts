import { Separator, type Theme, type Keybinding } from '@inquirer/core';
import type { PartialDeep } from '@inquirer/type';
type SelectTheme = {
    icon: {
        cursor: string;
    };
    style: {
        disabled: (text: string) => string;
        description: (text: string) => string;
        keysHelpTip: (keys: [key: string, action: string][]) => string | undefined;
    };
    i18n: {
        disabledError: string;
    };
    indexMode: 'hidden' | 'number';
    keybindings: ReadonlyArray<Keybinding>;
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
    choices: readonly (Separator | Value | Choice<Value>)[];
    pageSize?: number | undefined;
    loop?: boolean | undefined;
    default?: NoInfer<Value> | undefined;
    theme?: PartialDeep<Theme<SelectTheme>> | undefined;
}, context?: import("@inquirer/type").Context) => Promise<Value>;
export default _default;
export { Separator } from '@inquirer/core';
