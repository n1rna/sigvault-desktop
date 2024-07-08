import { SocketContext } from '@/components/providers/socket-provider';
import { AppStateContext } from '@/components/providers/appstate-provider';

import * as React from "react";


export const useSocket = () => React.useContext(SocketContext);
export const useAppState = () => React.useContext(AppStateContext);
