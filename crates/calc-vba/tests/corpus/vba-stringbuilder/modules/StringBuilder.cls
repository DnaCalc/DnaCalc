VERSION 1.0 CLASS
BEGIN
  MultiUse = -1  'True
END
Attribute VB_Name = "StringBuilder"
Attribute VB_GlobalNameSpace = False
Attribute VB_Creatable = False
Attribute VB_PredeclaredId = False
Attribute VB_Exposed = False
'@IgnoreModule AssignedByValParameter, UseMeaningfulName, ProcedureNotUsed
'@Folder("Class")
'@ModuleDescription("StringBuilder class.")

'------------------------------------------------------------------------------
' MIT License
'
' Copyright (c) 2025 Vincent van Geerestein
'
' Permission is hereby granted, free of charge, to any person obtaining a copy
' of this software and associated documentation files (the "Software"), to deal
' in the Software without restriction, including without limitation the rights
' to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
' copies of the Software, and to permit persons to whom the Software is
' furnished to do so, subject to the following conditions:
'
' The above copyright notice and this permission notice shall be included in all
' copies or substantial portions of the Software.
'
' THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
' IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
' FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
' AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
' LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
' OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
' SOFTWARE.
'------------------------------------------------------------------------------

Option Explicit

''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''
' Comments
''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''

' Author: Vincent van Geerestein
' E-mail: vincent@vangeerestein.com
' Description: StringBuilder class
' Add-in: RubberDuck (https://rubberduckvba.com/)
' Version: 2026.11.09
'
' For comparison see the StringBuilder class in .NET.
'
' Instead of appending a string to another string by using the VB & operator,
' the string is copied at the append position of a string buffer in a container.
' This methodology avoids performance degradation due to the excessive string
' memory allocations.
'
' Methods
' object.Append     Appends a value, converted to a string, to the string
' object.AppendAll  Appends any number of values, converted to strings
' object.Clear      Clears the object by initialization
' object.Flush      Flushes the string from the object
' object.Peek       Peeks the current string in the object
' object.Shorten    Shortens the string in the object by a number of characters
'
' Properties
' object.Count      Returns the length of the string in the object
' object.Mid        Equivalent to the VBA.Mid$ function and the Mid$ statement.
'
' The container starts with 1024 characters and doubles its size until it fits
' the newly appended string(s). A Long can't hold 2^31, so the container stops
' at 2^30 characters: beyond that Append raises error 6 (Overflow).
'
' Append takes one value as a Variant by reference, so a String variable is
' passed without a copy (a ByVal String parameter would copy it, which is slow).
' AppendAll takes any number of values in a ParamArray, which costs about twice
' as much per call. Both convert every value to a String as VBA does: a number
' or date by the regional settings, Null raises error 94, an object without a
' default member that returns a value raises error 450, and an array error 13.
' Empty strings are skipped.
'
' Mid works exactly like the VBA.Mid$ function and the Mid$ statement on the
' string in the object: Mid(start) and Mid(start, length) return a part of it,
' and Mid(start [, length]) = text replaces characters without changing the
' length of the string. Shorten with a negative number raises error 5.
'
' Timings (µs per piece) for building a string from n pieces of 10 characters,
' see BenchmarkStringBuilder
'
' n                                    100     1,000    10,000   100,000
' Append                              0.14      0.15      0.15      0.15
' AppendAll (one piece per call)      0.30      0.32      0.33      0.32
' AppendAll (five per call)           0.15      0.16      0.15      0.16
' Concatenation with &                0.05      0.14      1.85         -
' VBA.Join of a new String array      0.07      0.07      0.07      0.09
' VBA.Join, array grows               0.09      0.08      0.08      0.11
'
' Append and AppendAll take the same time per piece at any length of the
' string, whereas & copies the whole string built so far for every piece: from
' about 1,000 pieces of 10 characters StringBuilder is faster than &, and its
' advantage grows with the length (& with 100,000 pieces would take minutes).
' Most of the time is the call itself: Append is about twice as fast as
' AppendAll with one piece, and AppendAll with five pieces per call is as fast
' as Append per piece. A String array with VBA.Join is about twice as fast as
' Append for short pieces; most of its time is assigning the pieces to the
' array, not the Join. For pieces of 1,000 characters Append is faster.
'
' Typical usage of a StringBuilder object when the object name is not required:
'
' With New StringBuilder
'   .Append ...
'   .Append ...
'   ...
'   Dim Build As String
'   Build = .Flush
' End With

''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''
' Private declarations
''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''

' Selected VB errors.
Private Enum VBERROR
    vbErrorInvalidProcedureCall = 5
End Enum

' Wrapper for private data.
Private Type TPrivate
    Container As String
    Count As Long
End Type
Private this As TPrivate


''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''
' Class methods
''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''

Private Sub Class_Initialize()

    Const InitialLength As Long = 1024
    this.Container = Space$(InitialLength)
    this.Count = 0

End Sub


''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''
' Public methods
''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''

'@Description "Appends a value, converted to a string, to the string in the object."
'@Ignore ParameterCanBeByVal
Public Sub Append(ByRef value As Variant)
' ByRef on purpose: a String variable is then passed without a copy.

    ' Len and Mid$ convert the value to a string automatically.
    Dim ValueLength As Long: ValueLength = Len(value)

    ' Skip an empty string: Mid$ can't start beyond a full container.
    If ValueLength > 0 Then
        ' Check the container size and extend it if necessary.
        If Len(this.Container) < (this.Count + ValueLength) Then
            ExtendContainer this.Count + ValueLength
        End If

        ' Copy the string to the append position of the container.
        Mid$(this.Container, this.Count + 1) = value
        this.Count = this.Count + ValueLength
    End If

End Sub


'@Description "Appends the arguments, converted to strings, to the string in the object."
Public Sub AppendAll(ParamArray args() As Variant)
' All arguments are ByRef by definition of a ParamArray.

    Dim i As Long
    For i = 0 To UBound(args)
        ' Len and Mid$ convert the argument to a string automatically; copying it
        ' to a String variable first would take three times as long.
        Dim ArgLength As Long: ArgLength = Len(args(i))

        ' Skip an empty argument: Mid$ can't start beyond a full container.
        If ArgLength > 0 Then
            ' Check the container size and extend it if necessary.
            If Len(this.Container) < (this.Count + ArgLength) Then
                ExtendContainer this.Count + ArgLength
            End If

            ' Copy the argument to the append position of the container.
            Mid$(this.Container, this.Count + 1) = args(i)
            this.Count = this.Count + ArgLength
        End If
    Next

End Sub


'@Description "Returns the length of the string in the object."
Public Property Get Count() As Long

    Count = this.Count

End Property


'@Description "Flushes the string from the object."
Public Function Flush() As String

    Flush = Left$(this.Container, this.Count)
    this.Count = 0

End Function


'@Description "Peeks the string in the object."
Public Function Peek() As String

    Peek = Left$(this.Container, this.Count)

End Function


'@Description "Clears the object."
Public Sub Clear()

    Class_Initialize

End Sub


'@Description "Shortens the string in the object by an amount of characters."
Public Sub Shorten(Optional ByVal cut As Long = 1)

    ' A negative number would lengthen the string with characters of the
    ' container that are not part of it.
    If cut < 0 Then Err.Raise vbErrorInvalidProcedureCall

    If cut < this.Count Then
        this.Count = this.Count - cut
    Else
        this.Count = 0
    End If

End Sub


'@Description "Equivalent to VBA.Mid$ function and Mid$ statement for strings."
Public Property Get Mid(ByVal Start As Long, Optional ByVal Length As Variant) As String

     ' A negative length is invalid, also beyond the end of the string.
     If Not VBA.IsMissing(Length) Then
         If Length < 0 Then Err.Raise vbErrorInvalidProcedureCall
     End If

     If Start > this.Count Then
         Exit Property
     ElseIf VBA.IsMissing(Length) Then
         Length = this.Count - Start + 1
     ElseIf Length > this.Count - Start + 1 Then
         Length = this.Count - Start + 1
     End If

     Mid = VBA.Mid$(this.Container, Start, Length)

End Property
Public Property Let Mid(ByVal Start As Long, Optional ByVal Length As Variant, ByVal RHS As String)

    If Start > this.Count Then
        Err.Raise vbErrorInvalidProcedureCall
    ElseIf VBA.IsMissing(Length) Then
        Length = Len(RHS)
    ElseIf Length > this.Count - Start + 1 Then
        Length = this.Count - Start + 1
    End If

    Mid$(this.Container, Start, Length) = RHS

End Property


''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''
' Private methods
''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''

'@Description "Extends the container for building the string."
Private Sub ExtendContainer(ByVal TargetLength As Long)
' The size of the container is growing exponentially by a constant factor.

    Const GrowthFactor As Double = 2
    Dim Length As Long: Length = Len(this.Container)
    Do
        Length = GrowthFactor * Length
    Loop Until Length >= TargetLength

    this.Container = this.Container & VBA.Space$(Length - Len(this.Container))

End Sub
