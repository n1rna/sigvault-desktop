// components/windowTitlebar.js

import { useEffect, useState } from 'react'
import { Cross1Icon, MinusIcon } from "@radix-ui/react-icons";
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';


export function WindowTitlebar() {
    const [appWindow, setAppWindow] = useState<WebviewWindow>()

    // Import appWindow and save it inside the state for later usage
    async function setupAppWindow() {
        const appWindow = (await import('@tauri-apps/api/webviewWindow')).getCurrentWebviewWindow()
        setAppWindow(appWindow)
    }

    useEffect(() => {
        setupAppWindow()
    }, [])

    // These 3 functions will see the "appWindow" stored inside the state
    async function windowMinimize() {
        await appWindow?.minimize()
    }
    async function windowClose() {
        await appWindow?.close()
    }

    // Use "onClick" on buttons and call the 3 functions above
    return (
        <div data-tauri-drag-region className="bg-stone-800 flex justify-end top-0 left-0 right-0 fixed select-none h-8">
            <div className="inline-flex justify-center items-center h-8 w-8 text-white hover:bg-stone-600" id="titlebar-minimize">
                {<MinusIcon onClick={windowMinimize} />}
            </div>
            <div className="inline-flex justify-center items-center h-8 w-8 text-white hover:bg-stone-600" id="titlebar-close">
                {<Cross1Icon onClick={windowClose} />}
            </div>
        </div>
    )
}
