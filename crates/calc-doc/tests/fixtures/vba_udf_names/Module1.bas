Attribute VB_Name = "Module1"
Option Explicit
Public Function MyFunc(Optional ByVal x As Double = 1) As Double
Attribute MyFunc.VB_Description = "Measured MyFunc description"
Attribute MyFunc.VB_ProcData.VB_Invoke_Func = " \n14"
MyFunc = x + 100
End Function
Public Function SUM(Optional ByVal x As Double = 1) As Double
SUM = x + 200
End Function
Public Function ACOT(Optional ByVal x As Double = 1) As Double
ACOT = x + 300
End Function
Public Function Foo_2(Optional ByVal x As Double = 1) As Double
Foo_2 = x + 400
End Function
Public Function Café(Optional ByVal x As Double = 1) As Double
Café = x + 500
End Function
Public Function Ångström(Optional ByVal x As Double = 1) As Double
Ångström = x + 600
End Function
Public Function A1(Optional ByVal x As Double = 1) As Double
A1 = x + 700
End Function
Public Function XFD1(Optional ByVal x As Double = 1) As Double
XFD1 = x + 800
End Function
Public Function R1C1(Optional ByVal x As Double = 1) As Double
R1C1 = x + 900
End Function
Public Function CaseFunc(Optional ByVal x As Double = 1) As Double
CaseFunc = x + 1000
End Function
Public Function SameName(Optional ByVal x As Double = 1) As Double
SameName = x + 1100
End Function
Private Function SecretFunc(Optional ByVal x As Double = 1) As Double
SecretFunc = x + 1200
End Function

