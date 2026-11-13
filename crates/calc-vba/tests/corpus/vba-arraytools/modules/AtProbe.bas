Attribute VB_Name = "AtProbe"
Option Explicit
' Own worksheet wrapper (MIT, this repository) around the verbatim VBA-ArrayTools LibArrayTools module.

Private Function Show(ByVal v As Variant) As String
    Dim i As Long
    Dim s As String
    For i = LBound(v) To UBound(v)
        s = s & CStr(v(i)) & ","
    Next i
    Show = "(" & LBound(v) & ":" & UBound(v) & ")" & s
End Function

Private Function Show2D(ByVal v As Variant) As String
    Dim i As Long
    Dim j As Long
    Dim s As String
    For i = LBound(v, 1) To UBound(v, 1)
        For j = LBound(v, 2) To UBound(v, 2)
            s = s & CStr(v(i, j)) & ","
        Next j
        s = s & ";"
    Next i
    Show2D = s
End Function

Public Function AtSort(ByVal a As Variant, ByVal b As Variant, ByVal c As Variant, ByVal d As Variant, ByVal ascending As Boolean) As String
    AtSort = Show(LibArrayTools.Sort1DArray(Array(a, b, c, d), ascending))
End Function

Public Function AtSortText(ByVal a As String, ByVal b As String, ByVal c As String, ByVal caseSensitive As Boolean) As String
    AtSortText = Show(LibArrayTools.Sort1DArray(Array(a, b, c), True, True, caseSensitive))
End Function

Public Function AtReverse(ByVal a As Variant, ByVal b As Variant, ByVal c As Variant, ByVal d As Variant, ByVal group As Long) As String
    AtReverse = Show(LibArrayTools.Reverse1DArray(Array(a, b, c, d), group))
End Function

Public Function AtSlice(ByVal start As Long, ByVal length As Long, ByVal lowBound As Long) As String
    AtSlice = Show(LibArrayTools.Slice1DArray(Array("a", "b", "c", "d", "e"), start, length, lowBound))
End Function

Public Function AtMerge(ByVal lowBound As Long) As String
    AtMerge = Show(LibArrayTools.Merge1DArrays(Array(1, 2), Array("x", "y", "z"), lowBound))
End Function

Public Function AtSequence(ByVal n As Long, ByVal first As Double, ByVal stp As Double) As String
    Dim d() As Double
    d = LibArrayTools.Sequence1D(n, first, stp)
    AtSequence = Show(d)
End Function

Public Function AtRange(ByVal startValue As Long, ByVal endValue As Long, ByVal lowBound As Long) As String
    AtRange = Show(LibArrayTools.IntegerRange1D(startValue, endValue, lowBound))
End Function

Public Function AtUnique(ByVal a As Variant, ByVal b As Variant, ByVal c As Variant, ByVal d As Variant, ByVal e As Variant) As String
    AtUnique = Show(LibArrayTools.GetUniqueValues(Array(a, b, c, d, e)))
End Function

Public Function AtTranspose() As String
    Dim m As Variant
    m = LibArrayTools.OneDArrayTo2DArray(Array(1, 2, 3, 4, 5, 6), 3)
    AtTranspose = Show2D(m) & "|" & Show2D(LibArrayTools.TransposeArray(m))
End Function

Public Function AtDims() As String
    Dim m(1 To 2, 1 To 3) As Long
    AtDims = LibArrayTools.GetArrayDimsCount(Array(1, 2)) & "|" & LibArrayTools.GetArrayDimsCount(m) & "|" & LibArrayTools.GetArrayElemCount(m) & "|" & LibArrayTools.GetArrayElemCount(Array(7, 8, 9))
End Function

Public Function AtIterable() As String
    Dim c As New Collection
    AtIterable = LibArrayTools.IsIterable(Array(1)) & "|" & LibArrayTools.IsIterable(c) & "|" & LibArrayTools.IsIterable(5) & "|" & LibArrayTools.IsIterable("abc")
End Function

Public Function AtSort2D(ByVal col As Long) As String
    Dim m As Variant
    m = LibArrayTools.OneDArrayTo2DArray(Array(3, "c", 1, "a", 2, "b"), 2)
    AtSort2D = Show2D(LibArrayTools.Sort2DArray(m, col))
End Function

Public Function AtCollectionSort() As String
    Dim c As Collection
    Dim v As Variant
    Dim s As String
    Set c = LibArrayTools.Collection(5, 3, 9, 1)
    Set c = LibArrayTools.SortCollection(c)
    For Each v In c
        s = s & v & ","
    Next v
    AtCollectionSort = c.Count & ":" & s
End Function

Public Function AtOperatorText(ByVal t As String) As String
    AtOperatorText = LibArrayTools.GetConditionOperatorText(LibArrayTools.GetConditionOperator(t))
End Function

Public Function AtSwap(ByVal a As Variant, ByVal b As Variant) As String
    LibArrayTools.SwapValues a, b
    AtSwap = a & "|" & b
End Function

Public Function AtFilter(ByVal threshold As Double) As String
    Dim f(0 To 0) As LibArrayTools.FILTER_PAIR
    f(0) = LibArrayTools.CreateFilter(LibArrayTools.CONDITION_OPERATOR.opBigger, threshold)
    AtFilter = Show(LibArrayTools.Filter1DArray(Array(5, 1, 8, 3, 9), f))
End Function
