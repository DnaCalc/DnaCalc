Attribute VB_Name = "WasabiProbe"
Option Explicit
' Own worksheet wrapper (MIT, this repository) around Wasabi's public API. Only calls that cannot reach a
' network: unconnected-state queries, and WebSocketConnect with URLs its parser rejects (no DNS, no socket).

Public Function WsProbeConnect(ByVal url As String) As String
    Dim h As Long
    Dim ok As Boolean
    h = 12345
    ok = WebSocketConnect(url, h)
    WsProbeConnect = ok & "|" & h & "|" & WasabiGetErrorDescription(-5)
End Function

Public Function WsProbeHandleCount() As Long
    WsProbeHandleCount = UBound(WebSocketGetAllHandles()) - LBound(WebSocketGetAllHandles()) + 1
End Function

Public Function WsProbeCounts() As String
    WsProbeCounts = WebSocketGetConnectionCount() & "|" & TcpGetConnectionCount()
End Function

Public Function WsProbeQueries(ByVal h As Long) As String
    WsProbeQueries = TcpIsConnected(h) & "|" & WebSocketIsConnected(h) & "|" & TcpGetPendingBytes(h) & "|" & WebSocketGetPendingCount(h) & "|" & WebSocketGetQueueCapacity(h) & "|" & TcpGetPort(h) & "|" & WebSocketGetCloseCode(h) & "|" & TcpGetMode(h)
End Function
