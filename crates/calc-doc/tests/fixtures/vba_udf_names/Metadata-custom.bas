Attribute VB_Name = "Module1"
Public Function MetaFunc(Optional ByVal x As Double = 1) As Double
Attribute MetaFunc.VB_Description = "Description survives"
Attribute MetaFunc.VB_ProcData.VB_Invoke_Func = " \n14"
MetaFunc = x + 4000
End Function

Public Sub SetMetadataViaVba()
Application.MacroOptions Macro:="MetaFunc", Description:="VBA-set description", Category:=3, ArgumentDescriptions:=Array("VBA-set argument")
End Sub

