<?xml version="1.0" encoding="utf-8"?>
<CuttingControlProgram MeasurementSystem="Metric" Thickness="3">
  <Technology />
  <ApplicationFlat PlanWidth="600" PlanHeight="500" SheetWidth="1000" SheetHeight="500" />
  <SubPrograms>
    <SubProgram Id="1" Name="1" PartNumber="1" PartWidth="200" PartHeight="100">
      <ControlCommands>
        <SetOperationMode Id="2" Value="2" />
        <SetSensingSensor Id="3" Value="On" />
        <SetToolCorrection Id="4" Value="Right" />
        <MoveLineTo Id="5" X="78.5" Y="8.5" />
        <BeginCutOut Id="6" Contour="Unknown" />
        <LineTo Id="7" X="78.5" Y="20.5" />
        <LineTo Id="8" X="84.5" Y="20.5" />
        <LineTo Id="9" X="86.5" Y="18.5" />
        <LineTo Id="10" X="86.5" Y="10.5" />
        <LineTo Id="11" X="84.5" Y="8.5" />
        <LineTo Id="12" X="78.5" Y="8.5" />
        <EndCutOut Id="13" />
        <SetToolCorrection Id="14" Value="None" />
        <SetToolCorrection Id="15" Value="Right" />
        <MoveLineTo Id="16" X="98.5" Y="8.5" />
        <BeginCutOut Id="17" Contour="Unknown" />
        <LineTo Id="18" X="90.5" Y="8.5" />
        <LineTo Id="19" X="90.5" Y="20.5" />
        <LineTo Id="20" X="98.5" Y="20.5" />
        <EndCutOut Id="21" />
        <SetToolCorrection Id="22" Value="None" />
        <SetToolCorrection Id="23" Value="Right" />
        <MoveLineTo Id="24" X="90.5" Y="14.5" />
        <BeginCutOut Id="25" Contour="Unknown" />
        <LineTo Id="26" X="96.5" Y="14.5" />
        <EndCutOut Id="27" />
        <SetToolCorrection Id="28" Value="None" />
        <SetToolCorrection Id="29" Value="Right" />
        <MoveLineTo Id="30" X="102.5" Y="8.5" />
        <BeginCutOut Id="31" Contour="Unknown" />
        <LineTo Id="32" X="102.5" Y="20.5" />
        <LineTo Id="33" X="106.5" Y="14.5" />
        <LineTo Id="34" X="110.5" Y="20.5" />
        <LineTo Id="35" X="110.5" Y="8.5" />
        <EndCutOut Id="36" />
        <SetToolCorrection Id="37" Value="None" />
        <SetToolCorrection Id="38" Value="Right" />
        <MoveLineTo Id="39" X="116.5" Y="8.5" />
        <BeginCutOut Id="40" Contour="Unknown" />
        <LineTo Id="41" X="114.5" Y="10.5" />
        <LineTo Id="42" X="114.5" Y="18.5" />
        <LineTo Id="43" X="116.5" Y="20.5" />
        <LineTo Id="44" X="120.5" Y="20.5" />
        <LineTo Id="45" X="122.5" Y="18.5" />
        <LineTo Id="46" X="122.5" Y="10.5" />
        <LineTo Id="47" X="120.5" Y="8.5" />
        <LineTo Id="48" X="116.5" Y="8.5" />
        <EndCutOut Id="49" />
        <SetToolCorrection Id="50" Value="None" />
        <SetOperationMode Id="51" Value="3" />
        <SetToolCorrection Id="52" Value="Left" />
        <MoveLineTo Id="53" X="37.573654371" Y="52.288042095" />
        <BeginLeadIn Id="54" />
        <SetPiercingMode Id="55" Value="Fine" />
        <ExecPiercing Id="56" />
        <LineTo Id="57" X="33.489943997" Y="48.206787029" />
        <ArcTo Id="58" X="33.488526424" Y="43.492742034" SweepAngle="90" />
        <EndLeadIn Id="59" />
        <BeginCutOut Id="60" Contour="Interior" />
        <LineTo Id="61" X="34.733196779" Y="42.33030107" />
        <ArcTo Id="62" X="32.742887093" Y="44.189120557" SweepAngle="344.347826087" />
        <LineTo Id="63" X="33.488526424" Y="43.492742034" />
        <BeginLeadOut Id="64" />
        <LineTo Id="65" X="33.853944406" Y="43.151465462" />
        <ArcTo Id="66" X="38.562061859" Y="43.387796312" SweepAngle="90" />
        <LineTo Id="67" X="41.915901812" Y="47.096132539" />
        <EndLeadOut Id="68" />
        <EndCutOut Id="69" />
        <SetToolCorrection Id="70" Value="None" />
        <SetToolCorrection Id="71" Value="Left" />
        <MoveLineTo Id="72" X="104.8427" Y="55.3415" />
        <BeginLeadIn Id="73" />
        <SetPiercingMode Id="74" Value="Fine" />
        <ExecPiercing Id="75" />
        <LineTo Id="76" X="100.25" Y="55.3415" />
        <ArcTo Id="77" X="100.25" Y="45.5" SweepAngle="180" />
        <EndLeadIn Id="78" />
        <BeginCutOut Id="79" Contour="Interior" />
        <LineTo Id="80" X="120.5" Y="45.5" />
        <ArcTo Id="81" X="120.5" Y="55.5" SweepAngle="180" />
        <LineTo Id="82" X="80.5" Y="55.5" />
        <ArcTo Id="83" X="80.5" Y="45.5" SweepAngle="180" />
        <LineTo Id="84" X="100.25" Y="45.5" />
        <BeginLeadOut Id="85" />
        <LineTo Id="86" X="100.75" Y="45.5" />
        <ArcTo Id="87" X="100.75" Y="55.4" SweepAngle="180" />
        <LineTo Id="88" X="96.25" Y="55.4" />
        <EndLeadOut Id="89" />
        <EndCutOut Id="90" />
        <SetToolCorrection Id="91" Value="None" />
        <SetToolCorrection Id="92" Value="Left" />
        <MoveLineTo Id="93" X="157.573654371" Y="52.288042095" />
        <BeginLeadIn Id="94" />
        <SetPiercingMode Id="95" Value="Fine" />
        <ExecPiercing Id="96" />
        <LineTo Id="97" X="153.489943997" Y="48.206787029" />
        <ArcTo Id="98" X="153.488526424" Y="43.492742034" SweepAngle="90" />
        <EndLeadIn Id="99" />
        <BeginCutOut Id="100" Contour="Interior" />
        <LineTo Id="101" X="154.733196779" Y="42.33030107" />
        <ArcTo Id="102" X="152.742887093" Y="44.189120557" SweepAngle="344.347826087" />
        <LineTo Id="103" X="153.488526424" Y="43.492742034" />
        <BeginLeadOut Id="104" />
        <LineTo Id="105" X="153.853944406" Y="43.151465462" />
        <ArcTo Id="106" X="158.562061859" Y="43.387796312" SweepAngle="90" />
        <LineTo Id="107" X="161.915901812" Y="47.096132539" />
        <EndLeadOut Id="108" />
        <EndCutOut Id="109" />
        <SetToolCorrection Id="110" Value="None" />
        <SetToolCorrection Id="111" Value="Right" />
        <MoveLineTo Id="112" X="-6.5" Y="0.5" />
        <BeginLeadIn Id="113" />
        <SetPiercingMode Id="114" Value="Fine" />
        <ExecPiercing Id="115" />
        <LineTo Id="116" X="0.5" Y="0.5" />
        <EndLeadIn Id="117" />
        <BeginCutOut Id="118" Contour="Exterior" />
        <LineTo Id="119" X="200.5" Y="0.5" />
        <LineTo Id="120" X="200.5" Y="100.5" />
        <LineTo Id="121" X="0.5" Y="100.5" />
        <LineTo Id="122" X="0.5" Y="0.5" />
        <BeginLeadOut Id="123" />
        <LineTo Id="124" X="0.5" Y="-4.5" />
        <EndLeadOut Id="125" />
        <EndCutOut Id="126" />
        <SetToolCorrection Id="127" Value="None" />
      </ControlCommands>
    </SubProgram>
    <SubProgram Id="128" Name="ResidualSheetCuts" PartWidth="0" PartHeight="500">
      <ControlCommands>
        <SetOperationMode Id="129" Value="0" />
        <SetToolCorrection Id="130" Value="None" />
        <SetSensingSensor Id="131" Value="Off" />
        <SetToolCorrection Id="132" Value="Right" />
        <MoveLineTo Id="133" X="600" Y="0" />
        <BeginCutOut Id="134" Contour="Exterior" />
        <SetPiercingMode Id="135" Value="Fine" />
        <ExecPiercing Id="136" />
        <LineTo Id="137" X="600" Y="500" />
        <SetSensingSensor Id="138" Value="Off" />
        <SetSensingSensor Id="139" Value="On" />
        <EndCutOut Id="140" />
        <ExecHighPositioning Id="141" />
        <SetToolCorrection Id="142" Value="None" />
      </ControlCommands>
    </SubProgram>
  </SubPrograms>
  <ControlCommands>
    <SetClamps Id="143" Value="Off" />
    <CallSubProgram Id="144" SubProgramName="1" X="12" Y="12" Rotation="0" />
    <CallSubProgram Id="145" SubProgramName="ResidualSheetCuts" X="0" Y="0" Rotation="0" />
  </ControlCommands>
</CuttingControlProgram>
