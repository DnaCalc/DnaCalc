Attribute VB_Name = "JsonProbe"
Option Explicit

' Own code (MIT, this repository): worksheet entry points over VBA-JSON. Pure functions only.

Public Function JsonRoundTrip(ByVal s As String) As String
    JsonRoundTrip = JsonConverter.ConvertToJson(JsonConverter.ParseJson(s))
End Function

Public Function JsonKey(ByVal s As String, ByVal k As String) As Variant
    JsonKey = JsonConverter.ParseJson(s)(k)
End Function

Public Function JsonCount(ByVal s As String) As Long
    JsonCount = JsonConverter.ParseJson(s).Count
End Function

Public Function JsonItem(ByVal s As String, ByVal i As Long) As Variant
    JsonItem = JsonConverter.ParseJson(s)(i)
End Function
