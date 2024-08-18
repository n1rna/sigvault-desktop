import { useCallback, useState } from "react";

interface UsePersistProps<T> {
    name: string;
    value: T;
}

type SetPersistValueCallback<T> = ((arg0: T) => T);

const usePersist = <T>({ name, value }: UsePersistProps<T>): [T, (value: SetPersistValueCallback<T>) => void] => {
    const keyName = `persist/${name}`;

    const getFromStorage = <T>(name: string, defaultValue?: T) => {
        try {
            const val = JSON.parse(localStorage.getItem(name) + "");
            if (val !== null) {
                return val;
            } else {
                localStorage.setItem(name, JSON.stringify(defaultValue));
            }
        } catch {
            return defaultValue;
        }
    };

    const [state, setState] = useState<T>(getFromStorage<T>(keyName, value));

    const setValue = useCallback(
        (_value: SetPersistValueCallback<T>) => {
            setState((prev) => {
                const newVal = _value(prev);
                localStorage.setItem(keyName, JSON.stringify(newVal));
                return newVal;
             });
            return;
        },
        [keyName]
    );

    return [state, setValue];
};

export default usePersist;
