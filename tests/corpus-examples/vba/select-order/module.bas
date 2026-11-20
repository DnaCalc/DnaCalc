Attribute VB_Name = "Dc_select_order_ad3235f"
Option Explicit

Public Function DcExample() As Variant
    Dim result As String
    Select Case 5
        Case 2 To 6: result = "range"
        Case Is >= 5: result = "threshold"
        Case Else: result = "other"
    End Select
    DcExample = result
End Function
