<?xml version="1.0" encoding="utf-8"?>
<CuttingControlProgram MeasurementSystem="Metric" Thickness="3">
  <Technology />
  <ApplicationFlat PlanWidth="800" PlanHeight="500" SheetWidth="1000" SheetHeight="500" />
  <SubPrograms>
    <SubProgram Id="1" Name="1" PartNumber="1" PartWidth="200" PartHeight="100">
      <ControlCommands>
        <SetOperationMode Id="2" Value="3" />
        <SetSensingSensor Id="3" Value="On" />
        <SetToolCorrection Id="4" Value="Right" />
        <MoveLineTo Id="5" X="-7" Y="0" />
        <BeginLeadIn Id="6" />
        <SetPiercingMode Id="7" Value="Fine" />
        <ExecPiercing Id="8" />
        <LineTo Id="9" X="0" Y="0" />
        <EndLeadIn Id="10" />
        <BeginCutOut Id="11" Contour="Exterior" />
        <LineTo Id="12" X="100" Y="0" />
        <ExecMicrojoint Id="13" />
        <LineTo Id="14" X="200" Y="0" />
        <LineTo Id="15" X="200" Y="50" />
        <ExecMicrojoint Id="16" />
        <LineTo Id="17" X="200" Y="100" />
        <LineTo Id="18" X="100" Y="100" />
        <ExecMicrojoint Id="19" />
        <LineTo Id="20" X="0" Y="100" />
        <LineTo Id="21" X="0" Y="50" />
        <ExecMicrojoint Id="22" />
        <LineTo Id="23" X="0" Y="0" />
        <BeginLeadOut Id="24" />
        <LineTo Id="25" X="0" Y="-5" />
        <EndLeadOut Id="26" />
        <EndCutOut Id="27" />
        <SetToolCorrection Id="28" Value="None" />
      </ControlCommands>
    </SubProgram>
    <SubProgram Id="29" Name="2" PartNumber="3" PartWidth="240" PartHeight="160">
      <ControlCommands>
        <SetOperationMode Id="30" Value="2" />
        <SetToolCorrection Id="31" Value="Right" />
        <MoveLineTo Id="32" X="98" Y="12.8" />
        <BeginCutOut Id="33" Contour="Unknown" />
        <LineTo Id="34" X="98" Y="24.8" />
        <LineTo Id="35" X="104" Y="24.8" />
        <LineTo Id="36" X="106" Y="22.8" />
        <LineTo Id="37" X="106" Y="14.8" />
        <LineTo Id="38" X="104" Y="12.8" />
        <LineTo Id="39" X="98" Y="12.8" />
        <EndCutOut Id="40" />
        <SetToolCorrection Id="41" Value="None" />
        <SetToolCorrection Id="42" Value="Right" />
        <MoveLineTo Id="43" X="118" Y="12.8" />
        <BeginCutOut Id="44" Contour="Unknown" />
        <LineTo Id="45" X="110" Y="12.8" />
        <LineTo Id="46" X="110" Y="24.8" />
        <LineTo Id="47" X="118" Y="24.8" />
        <EndCutOut Id="48" />
        <SetToolCorrection Id="49" Value="None" />
        <SetToolCorrection Id="50" Value="Right" />
        <MoveLineTo Id="51" X="110" Y="18.8" />
        <BeginCutOut Id="52" Contour="Unknown" />
        <LineTo Id="53" X="116" Y="18.8" />
        <EndCutOut Id="54" />
        <SetToolCorrection Id="55" Value="None" />
        <SetToolCorrection Id="56" Value="Right" />
        <MoveLineTo Id="57" X="122" Y="12.8" />
        <BeginCutOut Id="58" Contour="Unknown" />
        <LineTo Id="59" X="122" Y="24.8" />
        <LineTo Id="60" X="126" Y="18.8" />
        <LineTo Id="61" X="130" Y="24.8" />
        <LineTo Id="62" X="130" Y="12.8" />
        <EndCutOut Id="63" />
        <SetToolCorrection Id="64" Value="None" />
        <SetToolCorrection Id="65" Value="Right" />
        <MoveLineTo Id="66" X="136" Y="12.8" />
        <BeginCutOut Id="67" Contour="Unknown" />
        <LineTo Id="68" X="134" Y="14.8" />
        <LineTo Id="69" X="134" Y="22.8" />
        <LineTo Id="70" X="136" Y="24.8" />
        <LineTo Id="71" X="140" Y="24.8" />
        <LineTo Id="72" X="142" Y="22.8" />
        <LineTo Id="73" X="142" Y="14.8" />
        <LineTo Id="74" X="140" Y="12.8" />
        <LineTo Id="75" X="136" Y="12.8" />
        <EndCutOut Id="76" />
        <SetToolCorrection Id="77" Value="None" />
        <SetOperationMode Id="78" Value="3" />
        <SetToolCorrection Id="79" Value="Left" />
        <MoveLineTo Id="80" X="29.103424528" Y="30.51763809" />
        <BeginLeadIn Id="81" />
        <SetPiercingMode Id="82" Value="Fine" />
        <ExecPiercing Id="83" />
        <LineTo Id="84" X="27.878679656" Y="29.292893219" />
        <ArcTo Id="85" X="27.878679656" Y="27.878679656" SweepAngle="90" />
        <EndLeadIn Id="86" />
        <BeginCutOut Id="87" Contour="Interior" />
        <ArcTo Id="88" X="27.228361402" Y="28.851949703" SweepAngle="337.5" />
        <LineTo Id="89" X="27.878679656" Y="27.878679656" />
        <BeginLeadOut Id="90" />
        <LineTo Id="91" X="28.294414463" Y="27.60089454" />
        <ArcTo Id="92" X="29.688864557" Y="27.836497883" SweepAngle="90" />
        <LineTo Id="93" X="30.692455282" Y="29.248165569" />
        <EndLeadOut Id="94" />
        <EndCutOut Id="95" />
        <SetToolCorrection Id="96" Value="None" />
        <SetOperationMode Id="97" Value="4" />
        <SetToolCorrection Id="98" Value="Left" />
        <MoveLineTo Id="99" X="28.505707546" Y="130.86273015" />
        <BeginLeadIn Id="100" />
        <SetPiercingMode Id="101" Value="Fine" />
        <ExecPiercing Id="102" />
        <LineTo Id="103" X="26.464466094" Y="128.821488698" />
        <ArcTo Id="104" X="26.464466094" Y="126.464466094" SweepAngle="90" />
        <EndLeadIn Id="105" />
        <BeginCutOut Id="106" Contour="Interior" />
        <ArcTo Id="107" X="25.380602337" Y="128.086582838" SweepAngle="337.5" />
        <LineTo Id="108" X="26.464466094" Y="126.464466094" />
        <BeginLeadOut Id="109" />
        <LineTo Id="110" X="26.8802009" Y="126.186680978" />
        <ArcTo Id="111" X="29.225519078" Y="126.421283194" SweepAngle="90" />
        <LineTo Id="112" X="31.05345272" Y="128.655559796" />
        <EndLeadOut Id="113" />
        <EndCutOut Id="114" />
        <SetToolCorrection Id="115" Value="None" />
        <SetToolCorrection Id="116" Value="Left" />
        <MoveLineTo Id="117" X="208.505707546" Y="130.86273015" />
        <BeginLeadIn Id="118" />
        <SetPiercingMode Id="119" Value="Fine" />
        <ExecPiercing Id="120" />
        <LineTo Id="121" X="206.464466094" Y="128.821488698" />
        <ArcTo Id="122" X="206.464466094" Y="126.464466094" SweepAngle="90" />
        <EndLeadIn Id="123" />
        <BeginCutOut Id="124" Contour="Interior" />
        <ArcTo Id="125" X="205.380602337" Y="128.086582838" SweepAngle="337.5" />
        <LineTo Id="126" X="206.464466094" Y="126.464466094" />
        <BeginLeadOut Id="127" />
        <LineTo Id="128" X="206.8802009" Y="126.186680978" />
        <ArcTo Id="129" X="209.225519078" Y="126.421283194" SweepAngle="90" />
        <LineTo Id="130" X="211.05345272" Y="128.655559796" />
        <EndLeadOut Id="131" />
        <EndCutOut Id="132" />
        <SetToolCorrection Id="133" Value="None" />
        <SetOperationMode Id="134" Value="3" />
        <SetToolCorrection Id="135" Value="Left" />
        <MoveLineTo Id="136" X="209.103424528" Y="30.51763809" />
        <BeginLeadIn Id="137" />
        <SetPiercingMode Id="138" Value="Fine" />
        <ExecPiercing Id="139" />
        <LineTo Id="140" X="207.878679656" Y="29.292893219" />
        <ArcTo Id="141" X="207.878679656" Y="27.878679656" SweepAngle="90.000000001" />
        <EndLeadIn Id="142" />
        <BeginCutOut Id="143" Contour="Interior" />
        <ArcTo Id="144" X="207.228361402" Y="28.851949703" SweepAngle="337.5" />
        <LineTo Id="145" X="207.878679656" Y="27.878679656" />
        <BeginLeadOut Id="146" />
        <LineTo Id="147" X="208.294414463" Y="27.60089454" />
        <ArcTo Id="148" X="209.688864557" Y="27.836497883" SweepAngle="90.000000001" />
        <LineTo Id="149" X="210.692455282" Y="29.248165569" />
        <EndLeadOut Id="150" />
        <EndCutOut Id="151" />
        <SetToolCorrection Id="152" Value="None" />
        <SetToolCorrection Id="153" Value="Right" />
        <MoveLineTo Id="154" X="-7" Y="0" />
        <BeginLeadIn Id="155" />
        <SetPiercingMode Id="156" Value="Fine" />
        <ExecPiercing Id="157" />
        <LineTo Id="158" X="0" Y="0" />
        <EndLeadIn Id="159" />
        <BeginCutOut Id="160" Contour="Exterior" />
        <LineTo Id="161" X="53.333333333" Y="0" />
        <ExecMicrojoint Id="162" />
        <LineTo Id="163" X="186.666666667" Y="0" />
        <ExecMicrojoint Id="164" />
        <LineTo Id="165" X="240" Y="0" />
        <LineTo Id="166" X="240" Y="80" />
        <ExecMicrojoint Id="167" />
        <LineTo Id="168" X="240" Y="160" />
        <LineTo Id="169" X="186.666666667" Y="160" />
        <ExecMicrojoint Id="170" />
        <LineTo Id="171" X="53.333333333" Y="160" />
        <ExecMicrojoint Id="172" />
        <LineTo Id="173" X="0" Y="160" />
        <LineTo Id="174" X="0" Y="80" />
        <ExecMicrojoint Id="175" />
        <LineTo Id="176" X="0" Y="0" />
        <BeginLeadOut Id="177" />
        <LineTo Id="178" X="0" Y="-5" />
        <EndLeadOut Id="179" />
        <EndCutOut Id="180" />
        <SetToolCorrection Id="181" Value="None" />
      </ControlCommands>
    </SubProgram>
    <SubProgram Id="182" Name="3" PartNumber="2" PartWidth="150" PartHeight="150">
      <ControlCommands>
        <SetOperationMode Id="183" Value="3" />
        <SetToolCorrection Id="184" Value="Left" />
        <MoveLineTo Id="185" X="64.090638242" Y="74.698901524" />
        <BeginLeadIn Id="186" />
        <SetPiercingMode Id="187" Value="Fine" />
        <ExecPiercing Id="188" />
        <LineTo Id="189" X="59.14015725" Y="69.749887689" />
        <ArcTo Id="190" X="59.138585297" Y="59.143286087" SweepAngle="90" />
        <EndLeadIn Id="191" />
        <BeginCutOut Id="192" Contour="Interior" />
        <LineTo Id="193" X="61.440720681" Y="57.044612386" />
        <ArcTo Id="194" X="58.372299363" Y="59.841848018" SweepAngle="349.411764706" />
        <LineTo Id="195" X="59.138585297" Y="59.143286087" />
        <BeginLeadOut Id="196" />
        <LineTo Id="197" X="59.508089756" Y="58.806438265" />
        <ArcTo Id="198" X="67.284357964" Y="58.97863801" SweepAngle="90" />
        <LineTo Id="199" X="70.74075272" Y="62.591577988" />
        <EndLeadOut Id="200" />
        <EndCutOut Id="201" />
        <SetToolCorrection Id="202" Value="None" />
        <SetToolCorrection Id="203" Value="Right" />
        <MoveLineTo Id="204" X="17.017243943" Y="17.017243943" />
        <BeginLeadIn Id="205" />
        <SetPiercingMode Id="206" Value="Fine" />
        <ExecPiercing Id="207" />
        <LineTo Id="208" X="21.966991411" Y="21.966991411" />
        <EndLeadIn Id="209" />
        <BeginCutOut Id="210" Contour="Exterior" />
        <ArcTo Id="211" X="17.024215998" Y="27.420503688" SweepAngle="354.375" />
        <LineTo Id="212" X="21.966991411" Y="21.966991411" />
        <BeginLeadOut Id="213" />
        <LineTo Id="214" X="22.337466974" Y="21.631211934" />
        <LineTo Id="215" X="18.82556051" Y="18.072207439" />
        <EndLeadOut Id="216" />
        <EndCutOut Id="217" />
        <SetToolCorrection Id="218" Value="None" />
      </ControlCommands>
    </SubProgram>
    <SubProgram Id="219" Name="ResidualSheetCuts" PartWidth="0" PartHeight="500">
      <ControlCommands>
        <SetOperationMode Id="220" Value="0" />
        <SetToolCorrection Id="221" Value="None" />
        <SetSensingSensor Id="222" Value="Off" />
        <SetToolCorrection Id="223" Value="Right" />
        <MoveLineTo Id="224" X="800" Y="0" />
        <BeginCutOut Id="225" Contour="Exterior" />
        <SetPiercingMode Id="226" Value="Fine" />
        <ExecPiercing Id="227" />
        <LineTo Id="228" X="800" Y="500" />
        <SetSensingSensor Id="229" Value="Off" />
        <SetSensingSensor Id="230" Value="On" />
        <EndCutOut Id="231" />
        <ExecHighPositioning Id="232" />
        <SetToolCorrection Id="233" Value="None" />
      </ControlCommands>
    </SubProgram>
  </SubPrograms>
  <ControlCommands>
    <SetClamps Id="234" Value="Off" />
    <CallSubProgram Id="235" SubProgramName="1" X="30" Y="30" Rotation="0" />
    <CallSubProgram Id="236" SubProgramName="2" X="30" Y="200" Rotation="0" />
    <CallSubProgram Id="237" SubProgramName="3" X="300" Y="30" Rotation="0" />
    <CallSubProgram Id="238" SubProgramName="ResidualSheetCuts" X="0" Y="0" Rotation="0" />
  </ControlCommands>
</CuttingControlProgram>
