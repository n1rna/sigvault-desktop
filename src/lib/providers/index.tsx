import { AppStateContext } from '@/lib/providers/appstate-provider';

import * as React from "react";


export const useAppState = () => React.useContext(AppStateContext);
