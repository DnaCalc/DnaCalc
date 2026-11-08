VERSION 1.0 CLASS
BEGIN
  MultiUse = -1  'True
END
Attribute VB_Name = "DnaCounter"
Attribute VB_GlobalNameSpace = False
Attribute VB_Creatable = False
Attribute VB_PredeclaredId = False
Attribute VB_Exposed = False
Option Explicit

Private total_ As Long

Public Sub Add(ByVal amount As Long)
    total_ = total_ + amount
End Sub

Public Property Get Total() As Long
    Total = total_
End Property
