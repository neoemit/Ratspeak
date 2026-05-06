package org.ratspeak.android

import android.os.ParcelUuid
import java.util.UUID

// Nordic UART Service UUIDs — identifies RNode devices.
// Matches ble_rnode.rs in rsReticulum.
object BleUuids {
    val NUS_SERVICE: UUID = UUID.fromString("6E400001-B5A3-F393-E0A9-E50E24DCCA9E")
    val NUS_RX_CHAR: UUID = UUID.fromString("6E400002-B5A3-F393-E0A9-E50E24DCCA9E")
    val NUS_TX_CHAR: UUID = UUID.fromString("6E400003-B5A3-F393-E0A9-E50E24DCCA9E")
    val CCCD: UUID        = UUID.fromString("00002902-0000-1000-8000-00805f9b34fb")

    val NUS_SERVICE_PARCEL: ParcelUuid = ParcelUuid(NUS_SERVICE)
}
