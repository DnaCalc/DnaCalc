Attribute VB_Name = "RiffProbe"
Option Explicit
' Own worksheet wrapper (MIT, this repository) around Riff's public API before RiffOpen: the engine is
' not initialised, so every call answers the 'not open' value and touches no audio device or file.

Public Function RiffProbePeaks() As String
    Dim l As Single, r As Single
    l = 5!: r = 7!
    RiffMasterGetPeak l, r
    RiffProbePeaks = l & "|" & r
End Function

Public Function RiffProbeLoadMemory() As Long
    Dim data() As Byte
    ReDim data(0 To 3)
    RiffProbeLoadMemory = RiffLoadFromMemory(data)
End Function

Public Function RiffProbeUnload(ByVal h As Long) As String
    RiffUnload h
    RiffProbeUnload = "unloaded " & h & " " & RiffIsInitialized
End Function

Public Function RiffProbeBusVolume(ByVal bus As Long) As Single
    RiffProbeBusVolume = RiffBusVolume(bus)
End Function

Public Function RiffProbeMasterVolume() As Single
    RiffProbeMasterVolume = RiffMasterVolume
End Function

Public Function RiffProbeSetMaster(ByVal v As Single) As String
    RiffMasterVolume = v
    RiffProbeSetMaster = "set " & RiffMasterVolume
End Function

Public Function RiffProbeVoice(ByVal v As Long) As String
    RiffProbeVoice = RiffVoiceIsPlaying(v) & "|" & RiffVoiceIsPaused(v) & "|" & RiffVoiceBus(v) & "|" & RiffVoiceVolume(v) & "|" & RiffVoiceBitDepth(v) & "|" & RiffVoicePitch(v)
End Function

Public Function RiffProbeClose() As String
    RiffClose
    RiffProbeClose = "closed " & RiffIsInitialized
End Function
