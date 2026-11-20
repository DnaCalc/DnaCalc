Attribute VB_Name = "Dc_instrrev_start_6f0cee7"
Option Explicit

Public Function DcExample() As Variant
    DcExample = Array(InStrRev("ana ana ana", "ana"), InStrRev("ana ana ana", "ana", 6))
End Function
