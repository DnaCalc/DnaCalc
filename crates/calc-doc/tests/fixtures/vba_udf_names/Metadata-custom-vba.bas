Attribute VB_Name = "Module1"
Public Sub SetCustomViaVba()
Application.MacroOptions Macro:="MetaFunc", Description:="Custom category description", Category:="DNA UDF custom", ArgumentDescriptions:=Array("Custom category argument")
End Sub

Public Function MetaFunc(Optional ByVal x As Double = 1) As Double
Attribute MetaFunc.VB_Description = "Custom category description"
Attribute MetaFunc.VB_ProcData.VB_Invoke_Func = " \n20"
MetaFunc = x + 4000
End Function

Public Sub SetMetadataViaVba()
Application.MacroOptions Macro:="MetaFunc", Description:="VBA-set description", Category:=3, ArgumentDescriptions:=Array("VBA-set argument")
End Sub

