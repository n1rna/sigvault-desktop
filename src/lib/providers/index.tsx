import { BackendContext } from '@/lib/providers/backend-provider';
import { AppStateContext } from '@/lib/providers/appstate-provider';

import * as React from "react";


export const useBackend = () => React.useContext(BackendContext);
export const useAppState = () => React.useContext(AppStateContext);
